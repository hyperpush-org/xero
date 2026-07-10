mod agent_coordination;
mod agent_definition;
pub mod browser;
mod desktop_control;
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
mod workflow_definition;
mod workspace_index;

use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
};

use serde::{de, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Manager, Runtime};
use time::{format_description::well_known::Rfc3339, Duration as TimeDuration, OffsetDateTime};
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
        BranchSummaryDto, BrowserControlPreferenceDto, CommandError, CommandErrorClass,
        CommandResult, RepositoryDiffScope, RepositoryStatusEntryDto,
        ResolvedAgentToolApplicationStyleDto, RuntimeAgentIdDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlStateDto, SoulSettingsDto,
    },
    db::project_store,
    runtime::redaction::find_prohibited_persistence_content,
    runtime::{AgentProviderConfig, AgentRunCancellationToken},
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
    AutonomousBrowserAction, AutonomousBrowserOutput, AutonomousBrowserRequest,
    BrowserExecutionContext, BrowserExecutor, UnavailableBrowserExecutor, AUTONOMOUS_TOOL_BROWSER,
};
pub(crate) use desktop_control::desktop_action_approval_id;
pub use desktop_control::{
    shutdown_desktop_control_sidecar, AutonomousDesktopActor, AutonomousDesktopApp,
    AutonomousDesktopCapabilities, AutonomousDesktopControlAction, AutonomousDesktopControlRequest,
    AutonomousDesktopControlStatusSnapshot, AutonomousDesktopControllerLock,
    AutonomousDesktopCursorState, AutonomousDesktopDisplay, AutonomousDesktopIceCandidate,
    AutonomousDesktopIceServer, AutonomousDesktopIceServerUrls, AutonomousDesktopMouseButton,
    AutonomousDesktopObserveAction, AutonomousDesktopObserveRequest,
    AutonomousDesktopPermissionGrant, AutonomousDesktopPermissionStatus,
    AutonomousDesktopPolicyCategory, AutonomousDesktopPolicyDecision, AutonomousDesktopPolicyTrace,
    AutonomousDesktopRegion, AutonomousDesktopScreenshot, AutonomousDesktopSessionDescription,
    AutonomousDesktopSidecarStatus, AutonomousDesktopStreamAction, AutonomousDesktopStreamQuality,
    AutonomousDesktopStreamRequest, AutonomousDesktopStreamState, AutonomousDesktopStreamStatus,
    AutonomousDesktopStreamTransport, AutonomousDesktopTextSensitivity, AutonomousDesktopToolError,
    AutonomousDesktopToolOutput, AutonomousDesktopToolStatus,
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
pub use workflow_definition::{
    AutonomousWorkflowDefinitionAction, AutonomousWorkflowDefinitionOutput,
    AutonomousWorkflowDefinitionRequest, AutonomousWorkflowDefinitionSummary,
    AUTONOMOUS_TOOL_WORKFLOW_DEFINITION,
};
pub use workspace_index::{
    AutonomousWorkspaceIndexAction, AutonomousWorkspaceIndexOutput, AutonomousWorkspaceIndexRequest,
};

pub const AUTONOMOUS_TOOL_READ: &str = "read";
pub const AUTONOMOUS_TOOL_READ_MANY: &str = "read_many";
pub const AUTONOMOUS_TOOL_RESULT_PAGE: &str = "result_page";
pub const AUTONOMOUS_TOOL_STAT: &str = "stat";
pub const AUTONOMOUS_TOOL_SEARCH: &str = "search";
pub const AUTONOMOUS_TOOL_FIND: &str = "find";
pub const AUTONOMOUS_TOOL_GIT_STATUS: &str = "git_status";
pub const AUTONOMOUS_TOOL_GIT_DIFF: &str = "git_diff";
pub const AUTONOMOUS_TOOL_TOOL_ACCESS: &str = "tool_access";
pub const AUTONOMOUS_TOOL_HARNESS_RUNNER: &str = "harness_runner";
pub const AUTONOMOUS_TOOL_EDIT: &str = "edit";
pub const AUTONOMOUS_TOOL_WRITE: &str = "write";
pub const AUTONOMOUS_TOOL_PATCH: &str = "patch";
pub const AUTONOMOUS_TOOL_COPY: &str = "copy";
pub const AUTONOMOUS_TOOL_FS_TRANSACTION: &str = "fs_transaction";
pub const AUTONOMOUS_TOOL_JSON_EDIT: &str = "json_edit";
pub const AUTONOMOUS_TOOL_TOML_EDIT: &str = "toml_edit";
pub const AUTONOMOUS_TOOL_YAML_EDIT: &str = "yaml_edit";
pub const AUTONOMOUS_TOOL_DELETE: &str = "delete";
pub const AUTONOMOUS_TOOL_RENAME: &str = "rename";
pub const AUTONOMOUS_TOOL_MKDIR: &str = "mkdir";
pub const AUTONOMOUS_TOOL_LIST: &str = "list";
pub const AUTONOMOUS_TOOL_LIST_TREE: &str = "list_tree";
pub const AUTONOMOUS_TOOL_DIRECTORY_DIGEST: &str = "directory_digest";
pub const AUTONOMOUS_TOOL_HASH: &str = "file_hash";
pub const AUTONOMOUS_TOOL_COMMAND: &str = "command";
pub const AUTONOMOUS_TOOL_COMMAND_SESSION_START: &str = "command_session_start";
pub const AUTONOMOUS_TOOL_COMMAND_SESSION_READ: &str = "command_session_read";
pub const AUTONOMOUS_TOOL_COMMAND_SESSION_STOP: &str = "command_session_stop";
pub const AUTONOMOUS_TOOL_COMMAND_PROBE: &str = "command_probe";
pub const AUTONOMOUS_TOOL_COMMAND_VERIFY: &str = "command_verify";
pub const AUTONOMOUS_TOOL_COMMAND_RUN: &str = "command_run";
pub const AUTONOMOUS_TOOL_COMMAND_SESSION: &str = "command_session";
pub const AUTONOMOUS_TOOL_HOST_COMMAND: &str = "host_command";
pub const AUTONOMOUS_TOOL_PROCESS_MANAGER: &str = "process_manager";
pub const AUTONOMOUS_TOOL_RUNTIME_WAIT: &str = "runtime_wait";
pub const AUTONOMOUS_TOOL_ACTION_REQUIRED: &str = "action_required";
pub const AUTONOMOUS_TOOL_SUGGEST_ROUTING: &str = "suggest_routing";
pub const AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS: &str = "system_diagnostics";
pub const AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE: &str = "system_diagnostics_observe";
pub const AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED: &str = "system_diagnostics_privileged";
pub const AUTONOMOUS_TOOL_MACOS_AUTOMATION: &str = "macos_automation";
pub const AUTONOMOUS_TOOL_DESKTOP_OBSERVE: &str = "desktop_observe";
pub const AUTONOMOUS_TOOL_DESKTOP_CONTROL: &str = "desktop_control";
pub const AUTONOMOUS_TOOL_DESKTOP_STREAM: &str = "desktop_stream";
pub const AUTONOMOUS_TOOL_MCP: &str = "mcp";
pub const AUTONOMOUS_TOOL_MCP_LIST: &str = "mcp_list";
pub const AUTONOMOUS_TOOL_MCP_READ_RESOURCE: &str = "mcp_read_resource";
pub const AUTONOMOUS_TOOL_MCP_GET_PROMPT: &str = "mcp_get_prompt";
pub const AUTONOMOUS_TOOL_MCP_CALL_TOOL: &str = "mcp_call_tool";
pub const AUTONOMOUS_TOOL_SUBAGENT: &str = "subagent";
pub const AUTONOMOUS_TOOL_REQUEST_SENSITIVE_INPUT: &str = "request_sensitive_input";
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
pub const AUTONOMOUS_TOOL_STALE_FILE_ERROR_CODE: &str = "autonomous_tool_stale_file";
pub const AUTONOMOUS_TOOL_EXPECTED_HASH_REQUIRED_CODE: &str =
    "autonomous_tool_expected_hash_required";

static SENSITIVE_INPUT_APPROVALS: OnceLock<Mutex<BTreeMap<String, JsonValue>>> = OnceLock::new();

pub(crate) fn store_sensitive_input_approval(action_id: &str, values: JsonValue) {
    let Ok(mut approvals) = sensitive_input_approvals().lock() else {
        return;
    };
    approvals.insert(action_id.to_string(), values);
}

fn take_sensitive_input_approval(action_id: &str) -> Option<JsonValue> {
    sensitive_input_approvals()
        .lock()
        .ok()
        .and_then(|mut approvals| approvals.remove(action_id))
}

fn sensitive_input_approvals() -> &'static Mutex<BTreeMap<String, JsonValue>> {
    SENSITIVE_INPUT_APPROVALS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(super) fn stale_file_error(
    operation: &str,
    path: &str,
    hash_field: &'static str,
    expected_hash: &str,
    current_hash: &str,
) -> CommandError {
    let details = json!({
        "path": path,
        "hashField": hash_field,
        "expectedHash": expected_hash.trim(),
        "currentHash": current_hash,
        "requiredAction": "re-read current file evidence, then retry with the new hash"
    });
    CommandError::user_fixable(
        AUTONOMOUS_TOOL_STALE_FILE_ERROR_CODE,
        format!(
            "Xero refused to {operation} `{path}` because `{hash_field}` no longer matches the current file. staleFile={details}"
        ),
    )
}

pub(super) fn expected_hash_required_error(
    operation: &str,
    path: &str,
    hash_field: &'static str,
    current_hash: &str,
) -> CommandError {
    let details = json!({
        "path": path,
        "hashField": hash_field,
        "currentHash": current_hash,
        "requiredAction": "read or hash the current file, then retry with this hash field"
    });
    CommandError::user_fixable(
        AUTONOMOUS_TOOL_EXPECTED_HASH_REQUIRED_CODE,
        format!(
            "Xero refused to {operation} existing file `{path}` without `{hash_field}`. staleFile={details}"
        ),
    )
}

pub(super) fn validate_sha256_hash(value: &str, field: &'static str) -> CommandResult<String> {
    let hash = value.trim();
    if hash.len() != 64
        || !hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(CommandError::user_fixable(
            "autonomous_tool_expected_hash_invalid",
            format!("Xero requires `{field}` to be a lowercase 64-character SHA-256 hex digest."),
        ));
    }
    Ok(hash.to_owned())
}

const DESKTOP_FEATURE_MASTER_ENV: &str = "XERO_COMPUTER_USE_DESKTOP_ENABLED";
const DESKTOP_FEATURE_OBSERVE_ENV: &str = "XERO_COMPUTER_USE_DESKTOP_OBSERVE_ENABLED";
const DESKTOP_FEATURE_CONTROL_ENV: &str = "XERO_COMPUTER_USE_DESKTOP_CONTROL_ENABLED";
const DESKTOP_FEATURE_STREAM_ENV: &str = "XERO_COMPUTER_USE_DESKTOP_STREAM_ENABLED";
const DESKTOP_FEATURE_ROLLOUT_PERCENT_ENV: &str = "XERO_COMPUTER_USE_DESKTOP_ROLLOUT_PERCENT";
const DESKTOP_FEATURE_ROLLOUT_ID_ENV: &str = "XERO_COMPUTER_USE_DESKTOP_ROLLOUT_ID";

const DEFAULT_READ_LINE_COUNT: usize = 200;
const MAX_READ_LINE_COUNT: usize = 400;
const MAX_TEXT_FILE_BYTES: usize = 512 * 1024;
const MAX_SEARCH_QUERY_CHARS: usize = 256;
const MAX_SEARCH_RESULTS: usize = 100;
const MAX_SEARCH_PREVIEW_CHARS: usize = 200;
pub(super) const DEFAULT_COMMAND_TIMEOUT_MS: u64 = 5_000;
const MAX_COMMAND_TIMEOUT_MS: u64 = 120_000;
const MAX_COMMAND_CAPTURE_BYTES: usize = 8 * 1024;
const MAX_COMMAND_EXCERPT_CHARS: usize = 2_000;
const DEFAULT_SUBAGENT_MAX_CHILD_AGENTS: usize = 6;
const DEFAULT_SUBAGENT_MAX_DEPTH: usize = 1;
const DEFAULT_SUBAGENT_MAX_CONCURRENT_CHILD_RUNS: usize = 3;
const DEFAULT_SUBAGENT_MAX_DELEGATED_TOOL_CALLS: usize = 40;
const DEFAULT_SUBAGENT_MAX_DELEGATED_TOKENS: u64 = 160_000;
const DEFAULT_SUBAGENT_MAX_DELEGATED_COST_MICROS: u64 = 250_000;
const MIN_RUNTIME_WAIT_DELAY_MS: u64 = 1_000;
const DEFAULT_RUNTIME_WAIT_POLL_INTERVAL_MS: u64 = 10_000;
const MAX_RUNTIME_WAIT_DELAY_MS: u64 = 30 * 60 * 1_000;
const MAX_RUNTIME_WAIT_DEADLINE_MS: u64 = 6 * 60 * 60 * 1_000;
const MAX_RUNTIME_WAIT_REASON_BYTES: usize = 400;
const MAX_RUNTIME_WAIT_RESUME_CONTEXT_BYTES: usize = 8 * 1024;
const MAX_ACTION_REQUIRED_DETAIL_BYTES: usize = 1_200;
const MAX_ACTION_REQUIRED_OPTIONS: usize = 20;
const MAX_ROUTE_REQUEST_REASON_BYTES: usize = 500;
const MAX_ROUTE_REQUEST_SUMMARY_BYTES: usize = 1_000;

const TOOL_ACCESS_CORE_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_READ,
    AUTONOMOUS_TOOL_READ_MANY,
    AUTONOMOUS_TOOL_RESULT_PAGE,
    AUTONOMOUS_TOOL_STAT,
    AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_FIND,
    AUTONOMOUS_TOOL_GIT_STATUS,
    AUTONOMOUS_TOOL_GIT_DIFF,
    AUTONOMOUS_TOOL_TOOL_ACCESS,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
    AUTONOMOUS_TOOL_ACTION_REQUIRED,
    AUTONOMOUS_TOOL_SUGGEST_ROUTING,
    AUTONOMOUS_TOOL_REQUEST_SENSITIVE_INPUT,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
    AUTONOMOUS_TOOL_WORKSPACE_INDEX,
    AUTONOMOUS_TOOL_AGENT_COORDINATION,
    AUTONOMOUS_TOOL_TODO,
    AUTONOMOUS_TOOL_LIST,
    AUTONOMOUS_TOOL_LIST_TREE,
    AUTONOMOUS_TOOL_DIRECTORY_DIGEST,
    AUTONOMOUS_TOOL_HASH,
];
const TOOL_ACCESS_MUTATION_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_EDIT,
    AUTONOMOUS_TOOL_WRITE,
    AUTONOMOUS_TOOL_PATCH,
    AUTONOMOUS_TOOL_COPY,
    AUTONOMOUS_TOOL_FS_TRANSACTION,
    AUTONOMOUS_TOOL_JSON_EDIT,
    AUTONOMOUS_TOOL_TOML_EDIT,
    AUTONOMOUS_TOOL_YAML_EDIT,
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
const TOOL_ACCESS_RUNTIME_WAIT_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_RUNTIME_WAIT,
    AUTONOMOUS_TOOL_ACTION_REQUIRED,
    AUTONOMOUS_TOOL_SUGGEST_ROUTING,
];
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
const TOOL_ACCESS_HOST_ADMIN_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_HOST_COMMAND];
const TOOL_ACCESS_REPOSITORY_RECON_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_READ,
    AUTONOMOUS_TOOL_READ_MANY,
    AUTONOMOUS_TOOL_RESULT_PAGE,
    AUTONOMOUS_TOOL_STAT,
    AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_FIND,
    AUTONOMOUS_TOOL_GIT_STATUS,
    AUTONOMOUS_TOOL_GIT_DIFF,
    AUTONOMOUS_TOOL_TOOL_ACCESS,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
    AUTONOMOUS_TOOL_ACTION_REQUIRED,
    AUTONOMOUS_TOOL_SUGGEST_ROUTING,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
    AUTONOMOUS_TOOL_WORKSPACE_INDEX,
    AUTONOMOUS_TOOL_LIST,
    AUTONOMOUS_TOOL_LIST_TREE,
    AUTONOMOUS_TOOL_DIRECTORY_DIGEST,
    AUTONOMOUS_TOOL_HASH,
    AUTONOMOUS_TOOL_COMMAND_PROBE,
    AUTONOMOUS_TOOL_CODE_INTEL,
    AUTONOMOUS_TOOL_LSP,
    AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
    AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
];
const TOOL_ACCESS_PLANNING_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_READ,
    AUTONOMOUS_TOOL_READ_MANY,
    AUTONOMOUS_TOOL_RESULT_PAGE,
    AUTONOMOUS_TOOL_STAT,
    AUTONOMOUS_TOOL_SEARCH,
    AUTONOMOUS_TOOL_FIND,
    AUTONOMOUS_TOOL_GIT_STATUS,
    AUTONOMOUS_TOOL_GIT_DIFF,
    AUTONOMOUS_TOOL_TOOL_ACCESS,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
    AUTONOMOUS_TOOL_ACTION_REQUIRED,
    AUTONOMOUS_TOOL_SUGGEST_ROUTING,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
    AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
    AUTONOMOUS_TOOL_WORKSPACE_INDEX,
    AUTONOMOUS_TOOL_LIST,
    AUTONOMOUS_TOOL_LIST_TREE,
    AUTONOMOUS_TOOL_DIRECTORY_DIGEST,
    AUTONOMOUS_TOOL_HASH,
    AUTONOMOUS_TOOL_TODO,
];
const TOOL_ACCESS_EMULATOR_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_EMULATOR];
const TOOL_ACCESS_DESKTOP_OBSERVE_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_DESKTOP_OBSERVE];
const TOOL_ACCESS_DESKTOP_CONTROL_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_DESKTOP_CONTROL];
const TOOL_ACCESS_DESKTOP_STREAM_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_DESKTOP_STREAM];
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
const TOOL_ACCESS_AGENT_DEFINITION_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_AGENT_DEFINITION,
    AUTONOMOUS_TOOL_WORKFLOW_DEFINITION,
];

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
        name: "host_admin",
        description: "Owner Admin gated host-wide commands for local workstation administration.",
        tools: TOOL_ACCESS_HOST_ADMIN_TOOLS,
        risk_class: "host_admin",
    },
    ToolAccessGroupDefinition {
        name: "process_manager",
        description: "Xero-owned process lifecycle, output, and external process observation/control surfaces.",
        tools: TOOL_ACCESS_PROCESS_MANAGER_TOOLS,
        risk_class: "process_control",
    },
    ToolAccessGroupDefinition {
        name: "runtime_wait",
        description: "Pause an owned-agent run for a bounded timer, durable process-poll wakeup, or bounded user-input prompt.",
        tools: TOOL_ACCESS_RUNTIME_WAIT_TOOLS,
        risk_class: "runtime_state",
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
        description: "macOS permissions, app/window inspection, screenshots, and bounded app/window automation.",
        tools: TOOL_ACCESS_MACOS_TOOLS,
        risk_class: "os_control",
    },
    ToolAccessGroupDefinition {
        name: "web_search_only",
        description: "Search the web for source discovery without exposing page fetch or browser-control schemas.",
        tools: TOOL_ACCESS_WEB_SEARCH_ONLY_TOOLS,
        risk_class: "network",
    },
    ToolAccessGroupDefinition {
        name: "web_fetch",
        description: "Fetch selected HTTP/HTTPS pages after search to inspect source content without exposing browser-control schemas.",
        tools: TOOL_ACCESS_WEB_FETCH_TOOLS,
        risk_class: "network",
    },
    ToolAccessGroupDefinition {
        name: "browser_observe",
        description: "Observe the Browser Automation Service with health/capabilities, page text/source, snapshots/versioned refs, waits/assertions, screenshots, console, network, accessibility, forms, frames, timeline, safety scans, and safe state reads.",
        tools: TOOL_ACCESS_BROWSER_OBSERVE_TOOLS,
        risk_class: "browser_observe",
    },
    ToolAccessGroupDefinition {
        name: "browser_control",
        description: "Control the Browser Automation Service with navigation, selector/ref actions, semantic actions, form fill, batch execution, cookie/storage writes, evidence export, annotations, recordings, and tab actions.",
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
        name: "desktop_observe",
        description: "Observe native desktop displays, windows, apps, app inventory, screenshots, cursor state, permissions, and sidecar health.",
        tools: TOOL_ACCESS_DESKTOP_OBSERVE_TOOLS,
        risk_class: "desktop_observe",
    },
    ToolAccessGroupDefinition {
        name: "desktop_control",
        description: "Approval-gated native desktop control through the desktop broker: pointer, keyboard, app/window, clipboard, and Accessibility actions.",
        tools: TOOL_ACCESS_DESKTOP_CONTROL_TOOLS,
        risk_class: "desktop_control",
    },
    ToolAccessGroupDefinition {
        name: "desktop_stream",
        description: "Start, stop, and inspect Computer Use desktop streaming, including degraded screenshot fallback state.",
        tools: TOOL_ACCESS_DESKTOP_STREAM_TOOLS,
        risk_class: "desktop_stream",
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
        description: "Draft, validate, list, save, update, archive, and clone registry-backed agent definitions and Workflow definitions.",
        tools: TOOL_ACCESS_AGENT_DEFINITION_TOOLS,
        risk_class: "definition_state",
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
            tool_summaries: Vec::new(),
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
    DesktopControl,
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
            Self::DesktopControl => "desktop_control",
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

    pub(crate) fn allowed_tool_names(&self, skill_tool_enabled: bool) -> BTreeSet<String> {
        deferred_tool_catalog(skill_tool_enabled)
            .into_iter()
            .map(|entry| entry.tool_name.to_owned())
            .filter(|tool| self.allows_tool(tool))
            .collect()
    }

    pub(crate) fn child_definition_policy_snapshot(&self, skill_tool_enabled: bool) -> JsonValue {
        json!({
            "allowedTools": self.allowed_tool_names(skill_tool_enabled),
            "externalServiceAllowed": self.external_service_allowed,
            "browserControlAllowed": self.browser_control_allowed,
            "skillRuntimeAllowed": self.skill_runtime_allowed,
            "subagentAllowed": self.subagent_allowed,
            "allowedSubagentRoles": &self.allowed_subagent_roles,
            "deniedSubagentRoles": &self.denied_subagent_roles,
            "commandAllowed": self.command_allowed,
            "destructiveWriteAllowed": self.destructive_write_allowed,
        })
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
                    "browser_control",
                    "device_control",
                    "external_service",
                    "skill_runtime",
                    "agent_delegation",
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
                external_service_allowed: true,
                browser_control_allowed: true,
                skill_runtime_allowed: true,
                subagent_allowed: true,
                allowed_subagent_roles: [
                    "engineer",
                    "debugger",
                    "planner",
                    "researcher",
                    "reviewer",
                    "browser",
                    "emulator",
                    "solana",
                    "database",
                ]
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
                denied_subagent_roles: BTreeSet::new(),
                command_allowed: true,
                destructive_write_allowed: true,
            },
            "agent_builder" => Self {
                allowed_effect_classes: ["observe", "runtime_state"]
                    .into_iter()
                    .map(ToOwned::to_owned)
                    .collect(),
                allowed_tools: [
                    AUTONOMOUS_TOOL_AGENT_DEFINITION.to_string(),
                    AUTONOMOUS_TOOL_WORKFLOW_DEFINITION.to_string(),
                ]
                .into(),
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
            "computer_use" => Self {
                allowed_effect_classes: [
                    "observe",
                    "runtime_state",
                    "write",
                    "destructive_write",
                    "command",
                    "process_control",
                    "browser_control",
                    "device_control",
                    "desktop_control",
                    "external_service",
                    "skill_runtime",
                    "agent_delegation",
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
                external_service_allowed: true,
                browser_control_allowed: true,
                skill_runtime_allowed: true,
                subagent_allowed: true,
                allowed_subagent_roles: [
                    "engineer",
                    "debugger",
                    "planner",
                    "researcher",
                    "reviewer",
                    "browser",
                    "emulator",
                    "solana",
                    "database",
                ]
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
                denied_subagent_roles: BTreeSet::new(),
                command_allowed: true,
                destructive_write_allowed: true,
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
            AutonomousDynamicToolRoute::ToolExtension { .. } => self.allows_tool(tool_name),
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
    TodoCompleted {
        todo_id: String,
    },
    ToolSucceeded {
        tool_names: BTreeSet<String>,
        min_count: usize,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct AutonomousAgentWorkflowRuntimeState {
    current_phase_id: String,
    tool_successes: BTreeMap<String, usize>,
    phase_failures: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AutonomousAgentWorkflowReplay {
    pub todo_items: BTreeMap<String, AutonomousTodoItem>,
    pub tool_successes: BTreeMap<String, usize>,
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

    pub(crate) fn initial_allowed_tools(&self) -> Option<BTreeSet<String>> {
        self.phase(&self.start_phase_id)
            .and_then(AutonomousAgentWorkflowPhase::registry_allowed_tools)
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

fn validate_subagent_workflow_structure(
    workflow_structure: Option<JsonValue>,
    role_policy: &AutonomousAgentToolPolicy,
) -> CommandResult<Option<JsonValue>> {
    let Some(workflow_structure) = workflow_structure else {
        return Ok(None);
    };
    let mut diagnostics = Vec::new();
    agent_definition::validate_workflow_structure(Some(&workflow_structure), &mut diagnostics);
    if let Some(diagnostic) = diagnostics.first() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_subagent_stage_invalid",
            format!(
                "Xero refused the child Stage configuration at `{}`: {}",
                diagnostic.path, diagnostic.message
            ),
        ));
    }
    let snapshot = json!({ "workflowStructure": workflow_structure.clone() });
    if AutonomousAgentWorkflowPolicy::from_definition_snapshot(&snapshot).is_none() {
        return Err(CommandError::user_fixable(
            "autonomous_tool_subagent_stage_invalid",
            "Xero could not compile the child Stage configuration into an executable policy.",
        ));
    }
    for tool in workflow_structure_referenced_tools(&workflow_structure) {
        if !role_policy.allows_tool(&tool) {
            return Err(CommandError::user_fixable(
                "autonomous_tool_subagent_stage_tool_incompatible",
                format!(
                    "Xero refused the child Stage configuration because tool `{tool}` is outside the selected child role policy."
                ),
            ));
        }
    }
    Ok(Some(workflow_structure))
}

fn workflow_structure_referenced_tools(workflow_structure: &JsonValue) -> BTreeSet<String> {
    let Some(phases) = workflow_structure
        .get("phases")
        .and_then(JsonValue::as_array)
    else {
        return BTreeSet::new();
    };
    let mut tools = BTreeSet::new();
    for phase in phases {
        if let Some(allowed_tools) = phase.get("allowedTools").and_then(JsonValue::as_array) {
            tools.extend(
                allowed_tools
                    .iter()
                    .filter_map(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|tool| !tool.is_empty())
                    .map(ToOwned::to_owned),
            );
        }
        if let Some(checks) = phase.get("requiredChecks").and_then(JsonValue::as_array) {
            for check in checks {
                collect_workflow_condition_tools(check, &mut tools);
            }
        }
        if let Some(branches) = phase.get("branches").and_then(JsonValue::as_array) {
            for branch in branches {
                if let Some(condition) = branch.get("condition") {
                    collect_workflow_condition_tools(condition, &mut tools);
                }
            }
        }
    }
    tools
}

fn collect_workflow_condition_tools(condition: &JsonValue, tools: &mut BTreeSet<String>) {
    if let Some(tool) = condition
        .get("toolName")
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|tool| !tool.is_empty())
    {
        tools.insert(tool.to_owned());
    }
    if let Some(tool_names) = condition.get("toolNames").and_then(JsonValue::as_array) {
        tools.extend(
            tool_names
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::trim)
                .filter(|tool| !tool.is_empty())
                .map(ToOwned::to_owned),
        );
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
                AUTONOMOUS_TOOL_ACTION_REQUIRED
                    | AUTONOMOUS_TOOL_SUGGEST_ROUTING
                    | AUTONOMOUS_TOOL_TODO
                    | AUTONOMOUS_TOOL_TOOL_SEARCH
                    | AUTONOMOUS_TOOL_TOOL_ACCESS
            )
    }

    fn registry_allowed_tools(&self) -> Option<BTreeSet<String>> {
        if self.allowed_tools.is_empty() {
            return None;
        }
        let mut allowed_tools = self.allowed_tools.clone();
        allowed_tools.insert(AUTONOMOUS_TOOL_ACTION_REQUIRED.to_owned());
        allowed_tools.insert(AUTONOMOUS_TOOL_SUGGEST_ROUTING.to_owned());
        allowed_tools.insert(AUTONOMOUS_TOOL_TODO.to_owned());
        allowed_tools.insert(AUTONOMOUS_TOOL_TOOL_SEARCH.to_owned());
        allowed_tools.insert(AUTONOMOUS_TOOL_TOOL_ACCESS.to_owned());
        Some(allowed_tools)
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
            "tool_succeeded" => {
                let mut tool_names = BTreeSet::new();
                if let Some(tool_name) = json_non_empty_string(object.get("toolName")) {
                    tool_names.insert(tool_name);
                }
                if let Some(items) = object.get("toolNames").and_then(JsonValue::as_array) {
                    tool_names.extend(
                        items
                            .iter()
                            .filter_map(|item| json_non_empty_string(Some(item))),
                    );
                }
                if tool_names.is_empty() {
                    return None;
                }
                Some(Self::ToolSucceeded {
                    tool_names,
                    min_count: object
                        .get("minCount")
                        .and_then(JsonValue::as_u64)
                        .filter(|count| *count > 0)
                        .unwrap_or(1) as usize,
                })
            }
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
                tool_names,
                min_count,
            } => {
                tool_names
                    .iter()
                    .map(|tool_name| state.tool_successes.get(tool_name).copied().unwrap_or(0))
                    .sum::<usize>()
                    >= *min_count
            }
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
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE
        | AUTONOMOUS_TOOL_DESKTOP_CONTROL
        | AUTONOMOUS_TOOL_DESKTOP_STREAM => {
            cfg!(any(
                target_os = "macos",
                target_os = "windows",
                target_os = "linux"
            )) && desktop_tool_available_by_rollout(tool)
        }
        AUTONOMOUS_TOOL_POWERSHELL => cfg!(target_os = "windows"),
        AUTONOMOUS_TOOL_HOST_COMMAND => cfg!(any(
            target_os = "macos",
            target_os = "windows",
            target_os = "linux"
        )),
        _ => true,
    }
}

pub(super) fn desktop_tool_available_by_rollout(tool: &str) -> bool {
    let tool_override = desktop_tool_feature_env(tool).and_then(|name| env::var(name).ok());
    let master = env::var(DESKTOP_FEATURE_MASTER_ENV).ok();
    let rollout_percent = env::var(DESKTOP_FEATURE_ROLLOUT_PERCENT_ENV).ok();
    let rollout_id = env::var(DESKTOP_FEATURE_ROLLOUT_ID_ENV)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(default_desktop_rollout_id);
    desktop_tool_available_from_rollout_values(
        tool,
        master.as_deref(),
        tool_override.as_deref(),
        rollout_percent.as_deref(),
        &rollout_id,
        desktop_tool_default_enabled(),
    )
}

fn desktop_tool_available_from_rollout_values(
    tool: &str,
    master: Option<&str>,
    tool_override: Option<&str>,
    rollout_percent: Option<&str>,
    rollout_id: &str,
    default_enabled: bool,
) -> bool {
    if let Some(enabled) = tool_override.and_then(parse_feature_bool) {
        return enabled;
    }
    if let Some(enabled) = master.and_then(parse_feature_bool) {
        return enabled;
    }
    if let Some(percent) = rollout_percent.and_then(parse_rollout_percent) {
        return desktop_rollout_bucket(rollout_id, tool) < percent;
    }
    default_enabled
}

fn desktop_tool_feature_env(tool: &str) -> Option<&'static str> {
    match tool {
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE => Some(DESKTOP_FEATURE_OBSERVE_ENV),
        AUTONOMOUS_TOOL_DESKTOP_CONTROL => Some(DESKTOP_FEATURE_CONTROL_ENV),
        AUTONOMOUS_TOOL_DESKTOP_STREAM => Some(DESKTOP_FEATURE_STREAM_ENV),
        _ => None,
    }
}

fn desktop_tool_default_enabled() -> bool {
    true
}

fn parse_feature_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" | "enabled" => Some(true),
        "0" | "false" | "no" | "off" | "disabled" => Some(false),
        _ => None,
    }
}

fn parse_rollout_percent(value: &str) -> Option<u8> {
    value
        .trim()
        .parse::<u8>()
        .ok()
        .filter(|percent| *percent <= 100)
}

fn desktop_rollout_bucket(rollout_id: &str, tool: &str) -> u8 {
    let mut hasher = Sha256::new();
    hasher.update(rollout_id.as_bytes());
    hasher.update(b":");
    hasher.update(tool.as_bytes());
    let digest = hasher.finalize();
    digest[0] % 100
}

fn default_desktop_rollout_id() -> String {
    env::var("XERO_INSTALLATION_ID")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("COMPUTERNAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .or_else(|| {
            env::var("HOSTNAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| format!("{}:{}", env::consts::OS, env::consts::ARCH))
}

pub fn tool_effect_class(tool_name: &str) -> AutonomousToolEffectClass {
    if tool_name.starts_with(AUTONOMOUS_DYNAMIC_MCP_TOOL_PREFIX) {
        return AutonomousToolEffectClass::ExternalService;
    }
    match tool_name {
        AUTONOMOUS_TOOL_READ
        | AUTONOMOUS_TOOL_READ_MANY
        | AUTONOMOUS_TOOL_RESULT_PAGE
        | AUTONOMOUS_TOOL_STAT
        | AUTONOMOUS_TOOL_SEARCH
        | AUTONOMOUS_TOOL_FIND
        | AUTONOMOUS_TOOL_GIT_STATUS
        | AUTONOMOUS_TOOL_GIT_DIFF
        | AUTONOMOUS_TOOL_LIST
        | AUTONOMOUS_TOOL_LIST_TREE
        | AUTONOMOUS_TOOL_DIRECTORY_DIGEST
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
        | AUTONOMOUS_TOOL_DESKTOP_OBSERVE
        | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE => AutonomousToolEffectClass::Observe,
        AUTONOMOUS_TOOL_TOOL_ACCESS
        | AUTONOMOUS_TOOL_ACTION_REQUIRED
        | AUTONOMOUS_TOOL_SUGGEST_ROUTING
        | AUTONOMOUS_TOOL_TODO
        | AUTONOMOUS_TOOL_REQUEST_SENSITIVE_INPUT
        | AUTONOMOUS_TOOL_RUNTIME_WAIT
        | AUTONOMOUS_TOOL_AGENT_COORDINATION
        | AUTONOMOUS_TOOL_AGENT_DEFINITION
        | AUTONOMOUS_TOOL_WORKFLOW_DEFINITION
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_UPDATE
        | AUTONOMOUS_TOOL_PROJECT_CONTEXT_REFRESH => AutonomousToolEffectClass::RuntimeState,
        AUTONOMOUS_TOOL_WRITE
        | AUTONOMOUS_TOOL_EDIT
        | AUTONOMOUS_TOOL_PATCH
        | AUTONOMOUS_TOOL_COPY
        | AUTONOMOUS_TOOL_FS_TRANSACTION
        | AUTONOMOUS_TOOL_JSON_EDIT
        | AUTONOMOUS_TOOL_TOML_EDIT
        | AUTONOMOUS_TOOL_YAML_EDIT
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
        | AUTONOMOUS_TOOL_HOST_COMMAND
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
        AUTONOMOUS_TOOL_DESKTOP_CONTROL | AUTONOMOUS_TOOL_DESKTOP_STREAM => {
            AutonomousToolEffectClass::DesktopControl
        }
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
    if matches!(
        tool_name,
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE
            | AUTONOMOUS_TOOL_DESKTOP_CONTROL
            | AUTONOMOUS_TOOL_DESKTOP_STREAM
            | AUTONOMOUS_TOOL_HOST_COMMAND
    ) {
        return agent_id == RuntimeAgentIdDto::ComputerUse;
    }
    if matches!(
        tool_name,
        AUTONOMOUS_TOOL_AGENT_DEFINITION | AUTONOMOUS_TOOL_WORKFLOW_DEFINITION
    ) {
        return agent_id == RuntimeAgentIdDto::AgentCreate;
    }
    if tool_name == AUTONOMOUS_TOOL_SUGGEST_ROUTING {
        return matches!(
            agent_id,
            RuntimeAgentIdDto::Ask
                | RuntimeAgentIdDto::Plan
                | RuntimeAgentIdDto::Engineer
                | RuntimeAgentIdDto::Debug
                | RuntimeAgentIdDto::Generalist
        );
    }
    match agent_id {
        RuntimeAgentIdDto::Engineer | RuntimeAgentIdDto::Debug | RuntimeAgentIdDto::Generalist => {
            true
        }
        RuntimeAgentIdDto::Plan => TOOL_ACCESS_PLANNING_TOOLS.contains(&tool_name),
        RuntimeAgentIdDto::Crawl => TOOL_ACCESS_REPOSITORY_RECON_TOOLS.contains(&tool_name),
        RuntimeAgentIdDto::ComputerUse => true,
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
    if tool_name == AUTONOMOUS_TOOL_SUGGEST_ROUTING {
        return tool_allowed_for_runtime_agent(agent_id, tool_name);
    }
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
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::ComputerUse, tool_name) {
        agents.push(RuntimeAgentIdDto::ComputerUse.as_str());
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
    if tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Generalist, tool_name) {
        agents.push(RuntimeAgentIdDto::Generalist.as_str());
    }
    agents
}

pub fn deferred_tool_catalog(skill_tool_enabled: bool) -> Vec<AutonomousToolCatalogEntry> {
    let mut catalog = vec![
        catalog_entry(
            AUTONOMOUS_TOOL_READ,
            "core",
            "Read a repo-relative file or bounded directory listing as text, image preview, binary metadata, byte range, or line-hash anchored text; non-informative sidecars are skipped.",
            &[
                "file",
                "directory",
                "inspect",
                "read",
                "line_hash",
                "image",
                "binary",
            ],
            &[
                "path",
                "systemPath",
                "mode",
                "startLine",
                "lineCount",
                "maxBytesPerFile",
                "byteOffset",
                "byteCount",
                "includeLineHashes",
            ],
            &[
                "Read src/lib.rs with line hashes before editing.",
                "Read . to inspect the imported repository root.",
                "Inspect an image preview in the imported repo.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_READ_MANY,
            "core",
            "Read a bounded ordered set of small repo-relative files with per-file errors, total byte caps, and non-informative sidecar omissions.",
            &["file", "inspect", "read", "batch", "line_hash"],
            &[
                "paths",
                "mode",
                "startLine",
                "lineCount",
                "maxBytesPerFile",
                "maxTotalBytes",
                "includeLineHashes",
            ],
            &[
                "Read package.json and tsconfig.json in one bounded call.",
                "Read several small source files with line hashes before planning edits.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_RESULT_PAGE,
            "core",
            "Read a bounded continuation slice from a tool artifact stored under this project's app-data tool-artifacts directory.",
            &["artifact", "continuation", "pagination", "read", "app_data"],
            &["artifactPath", "byteOffset", "maxBytes"],
            &[
                "Read the next slice from a command, patch, MCP, or manifest artifact path.",
                "Use nextByteOffset from result_page to continue a large artifact without rerunning the original tool.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_STAT,
            "core",
            "Inspect repo-relative path metadata without reading file content, including missing paths, symlinks, optional small-file hashes, and optional git status.",
            &[
                "file",
                "metadata",
                "stat",
                "exists",
                "symlink",
                "permissions",
                "hash",
                "git",
            ],
            &[
                "path",
                "followSymlinks",
                "includeGitStatus",
                "includeHash",
                "strict",
            ],
            &[
                "Check whether a path exists before deciding to read it.",
                "Inspect file size and hash without loading content into the model.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_LIST_TREE,
            "core",
            "Return a compact deterministic repo-relative directory tree with depth, entry, ignore, permission, filter, and sidecar omission counts.",
            &["directory", "tree", "inspect", "list", "git"],
            &[
                "path",
                "maxDepth",
                "maxEntries",
                "includeGlobs",
                "excludeGlobs",
                "includeGitStatus",
                "showOmitted",
            ],
            &[
                "Show a compact tree under src.",
                "Map a directory with git status and omission counts before planning edits.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_DIRECTORY_DIGEST,
            "core",
            "Compute a deterministic digest for a repo-relative directory or file set with omission counts, sidecar filtering, and a compact manifest.",
            &["directory", "digest", "hash", "guard", "manifest"],
            &[
                "path",
                "includeGlobs",
                "excludeGlobs",
                "maxFiles",
                "hashMode",
            ],
            &[
                "Compute a metadata-only digest for . before a repo-wide recursive operation.",
                "Compute a metadata-only digest for src before a recursive operation.",
                "Compute a content-hash digest for a generated file set.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_SEARCH,
            "core",
            "Search repo-scoped files with regex or literal matching, globs, context lines, hidden/ignored controls, sidecar filtering, and deterministic capped results.",
            &["file", "search", "regex", "grep", "ripgrep", "code"],
            &[
                "query",
                "path",
                "regex",
                "ignoreCase",
                "includeHidden",
                "includeIgnored",
                "includeGlobs",
                "excludeGlobs",
                "contextLines",
                "maxResults",
            ],
            &[
                "Search for a symbol before editing.",
                "Find TODO references with context lines.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_FIND,
            "core",
            "Find glob/pattern matches in repo-scoped files with optional bounded recursion depth and sidecar filtering.",
            &["file", "glob", "find", "tree"],
            &[
                "pattern",
                "path",
                "maxDepth",
                "includeHidden",
                "includeIgnored",
            ],
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
            "Search source-cited durable project records, approved memory, handoffs, decisions, constraints, questions, and blockers with freshness evidence. Context package inspection is diagnostic-only and must not be used for ordinary project overview, coding, planning, or debugging.",
            &[
                "context",
                "memory",
                "records",
                "handoff",
                "retrieval",
                "citations",
            ],
            &[
                "action",
                "query",
                "recordKinds",
                "memoryKinds",
                "tags",
                "relatedPaths",
                "includeHistorical",
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
            &["action", "recordId", "memoryId", "includeHistorical"],
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
            &[
                "Request command_readonly before running tests.",
                "Activate solana_alt after tool_search finds it.",
            ],
            "registry_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_TOOL_SEARCH,
            "core",
            "Search deferred tool capabilities before requesting activation.",
            &[
                "tool",
                "search",
                "discovery",
                "catalog",
                "bm25",
                "capability",
            ],
            &["query", "limit"],
            &[
                "Search for address lookup table tools.",
                "Find the smallest browser observation capability.",
            ],
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
            AUTONOMOUS_TOOL_WORKFLOW_DEFINITION,
            "agent_builder",
            "Draft, validate, list, get, save, and update registry-backed Workflow definitions in app-data-backed state.",
            &[
                "workflow",
                "definition",
                "multi_agent",
                "registry",
                "validation",
                "app_data",
            ],
            &["action", "projectId", "workflowId", "definition"],
            &[
                "Validate a multi-agent Workflow definition before saving.",
                "Save an approved Workflow definition after operator approval.",
            ],
            "workflow_definition_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_ENVIRONMENT_CONTEXT,
            "environment",
            "Read compact, redacted developer-environment facts from the app-global environment profile. Also discoverable as fetch_dev_tools for installed developer tool availability.",
            &[
                "environment",
                "fetch_dev_tools",
                "dev tools",
                "developer tools",
                "tool availability",
                "environment profile",
                "first run",
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
            AUTONOMOUS_TOOL_ACTION_REQUIRED,
            "core",
            "Pause the owned-agent run and ask the user for bounded non-sensitive input through the transcript UI.",
            &[
                "user input",
                "clarify",
                "choice",
                "single choice",
                "multi choice",
                "technology stack",
                "preference",
                "question",
            ],
            &[
                "title",
                "detail",
                "answerShape",
                "promptKind",
                "options",
                "intendedUse",
            ],
            &[
                "Ask the user to choose one technology stack before implementation.",
                "Ask for multiple independent preferences when a design choice is material.",
            ],
            "runtime_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_SUGGEST_ROUTING,
            "core",
            "Request a policy-validated agent switch and emit a durable typed route event.",
            &[
                "route",
                "routing",
                "agent switch",
                "handoff",
                "specialist",
                "runtime event",
            ],
            &[
                "targetKind",
                "targetAgentId",
                "targetAgentDefinitionId",
                "targetAgentDefinitionVersion",
                "reason",
                "summary",
            ],
            &[
                "Suggest built-in Engineer when the current agent cannot perform requested edits.",
                "Suggest an allowlisted custom agent and require explicit user confirmation.",
            ],
            "runtime_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_REQUEST_SENSITIVE_INPUT,
            "core",
            "Request secrets or sensitive configuration from the user through the dedicated redacted input flow.",
            &[
                "secret",
                "sensitive input",
                "credentials",
                "env",
                "api key",
                "token",
                "user input",
            ],
            &["purpose", "intendedUse", "fields", "allowPartial"],
            &[
                "Ask for an API key before creating an env file.",
                "Request optional local-service credentials without putting values in chat.",
            ],
            "runtime_state",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_LIST,
            "core",
            "List repo-scoped files while omitting non-informative OS/editor/cache sidecars.",
            &["file", "list", "tree", "directory"],
            &["path", "maxDepth"],
            &["List top-level project directories."],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_HASH,
            "core",
            "Hash a repo-relative file, directory, or matched file set with SHA-256, excluding non-informative sidecars in file-set mode.",
            &["file", "directory", "hash", "sha256", "stale_write"],
            &["path", "recursive", "includeGlobs", "excludeGlobs", "maxFiles"],
            &[
                "Hash a file before guarded mutation.",
                "Hash . as a file-set digest before repo-wide planning.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_WRITE,
            "mutation",
            "Create a UTF-8 text file or replace an existing file with expected-hash protection.",
            &["file", "write", "create", "replace"],
            &["path", "content"],
            &["Create a new generated source file."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_EDIT,
            "mutation",
            "Apply an exact expected-text line-range edit with mandatory owned-agent file hash anchors; non-empty replacements keep line boundaries when the final newline is omitted.",
            &["file", "edit", "line", "expected_text", "hash_guard"],
            &[
                "path",
                "startLine",
                "endLine",
                "expected",
                "replacement",
                "expectedHash",
                "startLineHash",
                "endLineHash",
            ],
            &["Replace a small function body after reading it."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PATCH,
            "mutation",
            "Apply a canonical UTF-8 text patch with preview, expected-hash guards, exact diagnostics, and multi-file support.",
            &[
                "file",
                "patch",
                "replace",
                "exact_text",
                "multi_file",
                "preview",
                "hash_guard",
            ],
            &[
                "path",
                "search",
                "replace",
                "replaceAll",
                "expectedHash",
                "preview",
                "operations",
            ],
            &[
                "Preview a multi-file patch before writing.",
                "Replace an exact import statement with an expected hash.",
            ],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COPY,
            "mutation",
            "Copy a repo-relative file or directory with preview and hash/digest guards.",
            &["file", "copy", "directory", "preview", "hash_guard"],
            &[
                "from",
                "to",
                "recursive",
                "expectedSourceHash",
                "expectedSourceDigest",
                "overwrite",
                "expectedTargetHash",
                "preview",
            ],
            &["Preview a guarded copy before writing."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_FS_TRANSACTION,
            "mutation",
            "Apply a guarded multi-step filesystem transaction with preview, validation, and rollback attempts.",
            &[
                "file",
                "transaction",
                "multi_file",
                "preview",
                "hash_guard",
                "rollback",
            ],
            &[
                "operations",
                "preview",
                "stopOnFirstError",
                "expectedHash",
                "expectedDigest",
                "expectedSourceDigest",
            ],
            &[
                "Preview a create/edit/rename plan before writing.",
                "Apply multiple guarded file mutations with rollback attempts on partial failure.",
            ],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_JSON_EDIT,
            "mutation",
            "Apply parser-backed JSON edits with preview and expected-hash guards.",
            &["file", "json", "structured_edit", "preview", "hash_guard"],
            &[
                "path",
                "operations",
                "expectedHash",
                "formattingMode",
                "preview",
            ],
            &["Set or delete a package.json key without string search/replace."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_TOML_EDIT,
            "mutation",
            "Apply parser-backed TOML edits with preview and expected-hash guards.",
            &["file", "toml", "structured_edit", "preview", "hash_guard"],
            &[
                "path",
                "operations",
                "expectedHash",
                "formattingMode",
                "preview",
            ],
            &["Set a Cargo.toml package or dependency key with parser validation."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_YAML_EDIT,
            "mutation",
            "Apply parser-backed YAML edits with preview and expected-hash guards.",
            &["file", "yaml", "structured_edit", "preview", "hash_guard"],
            &[
                "path",
                "operations",
                "expectedHash",
                "formattingMode",
                "preview",
            ],
            &["Set a workflow YAML field with parser validation."],
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
            "Run a narrowly allowlisted repo-scoped read-only discovery command. Use command_run, not command_probe, for package-manager create/install/add/update, scaffolding, generators, builds, or setup.",
            &["command", "probe", "diagnostic", "git", "rg", "metadata"],
            &["argv", "cwd", "timeoutMs"],
            &["Run git status or cargo metadata for local discovery."],
            "command_probe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_VERIFY,
            "command",
            "Run a narrowly allowlisted repo-scoped verification command for tests, checks, lint, build, or formatting verification. Use command_run for setup, scaffolding, generators, and package-manager create/install/add/update commands.",
            &[
                "command", "verify", "test", "lint", "build", "cargo", "npm", "pnpm",
            ],
            &["argv", "cwd", "timeoutMs"],
            &[
                "Run cargo test for the changed crate.",
                "Run pnpm test for a scoped package.",
            ],
            "command_verify",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_RUN,
            "command",
            "Run a repo-scoped command that is not covered by probe or verification policy.",
            &[
                "command",
                "shell",
                "run",
                "script",
                "setup",
                "scaffold",
                "vite",
                "npm",
                "pnpm",
                "install",
            ],
            &["argv", "cwd", "timeoutMs"],
            &[
                "Run a one-off repo-scoped helper after approval policy allows it.",
                "Run npm create vite@latest or another scaffold command with normal command approval.",
            ],
            "command",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_HOST_COMMAND,
            "host_admin",
            "Run a host-wide workstation administration command when local Owner Admin mode is active, with preview, approval, and audit metadata.",
            &[
                "command",
                "host",
                "admin",
                "owner_admin",
                "shell",
                "powershell",
                "brew",
                "winget",
                "service",
            ],
            &[
                "argv",
                "cwd",
                "timeoutMs",
                "preview",
                "previewToken",
                "reason",
                "rollbackHints",
            ],
            &[
                "Preview a package-manager command before running it in Owner Admin mode.",
                "Run a local service-management command after owner approval.",
            ],
            "host_admin",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_COMMAND_SESSION,
            "command",
            "Start, read, or stop a repo-scoped long-running command session.",
            &["command", "session", "long_running", "dev_server", "watch"],
            &[
                "action",
                "argv",
                "cwd",
                "timeoutMs",
                "sessionId",
                "afterSequence",
                "maxBytes",
            ],
            &["Start a dev server or watcher, then read and stop it."],
            "long_running_process",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_PROCESS_MANAGER,
            "process_manager",
            "Manage Xero-owned long-running, interactive, grouped, restartable, and async-job processes, plus system process visibility and approval-gated external signaling.",
            &[
                "process",
                "pty",
                "async",
                "background",
                "output",
                "signal",
                "port",
            ],
            &[
                "action",
                "processId",
                "groupId",
                "argv",
                "input",
                "timeoutMs",
                "maxBytes",
            ],
            &[
                "Start an async build and await completion.",
                "Inspect system ports.",
            ],
            "process_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_RUNTIME_WAIT,
            "runtime_wait",
            "Pause an owned-agent run for a bounded timer or durable process-poll wakeup that automatically resumes later.",
            &["wait", "timer", "poll", "resume", "scheduler", "process"],
            &[
                "kind",
                "delayMs",
                "processId",
                "pollIntervalMs",
                "deadlineMs",
                "outputPattern",
                "reason",
                "resumeContext",
            ],
            &[
                "Wait 10 seconds before checking again.",
                "Poll an async process until it exits or the deadline is reached.",
            ],
            "runtime_state",
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
            &[
                "diagnostics",
                "process",
                "sample",
                "accessibility",
                "macos",
                "approval",
            ],
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
            "macOS app/system automation: check permissions, list/launch/activate/quit apps, list/focus windows, and capture screenshots.",
            &[
                "macos",
                "desktop",
                "window",
                "app",
                "screenshot",
                "permission",
            ],
            &["action", "bundleId", "appName", "windowId", "target"],
            &["List running apps.", "Capture a screenshot."],
            "os_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
            "desktop_observe",
            "Observe native desktop state through the Computer Use desktop broker: permissions, displays, windows, apps, app inventory/launch targets, notifications, foreground state, screenshots, cursor state, OCR/Accessibility, clipboard text/HTML/RTF/image/files, browser/terminal bridge affordances, and health.",
            &[
                "desktop",
                "computer_use",
                "observe",
                "display",
                "window",
                "screenshot",
                "cursor",
                "permission",
            ],
            &[
                "action",
                "displayId",
                "windowId",
                "region",
                "x",
                "y",
                "includeData",
                "maxBytes",
            ],
            &[
                "List displays before selecting a screenshot target.",
                "Capture a screenshot of a selected display.",
            ],
            "desktop_observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            "desktop_control",
            "Native desktop control through the Computer Use broker: pointer movement, press-hold-release drag, click, scroll, keyboard, text entry, clipboard text/HTML/RTF/image/files, app/window, re-resolved Accessibility elements, menu, Dock/status item/file dialog helpers, and cancel actions.",
            &[
                "desktop",
                "computer_use",
                "control",
                "mouse",
                "keyboard",
                "accessibility",
                "audit",
            ],
            &[
                "action",
                "displayId",
                "windowId",
                "appName",
                "bundleId",
                "elementId",
                "x",
                "y",
                "sourceWidth",
                "sourceHeight",
                "toX",
                "toY",
                "deltaX",
                "deltaY",
                "width",
                "height",
                "includeData",
                "maxBytes",
                "mediaType",
                "imageDataBase64",
                "filePaths",
                "button",
                "clicks",
                "key",
                "keys",
                "text",
                "html",
                "rtf",
                "altText",
                "targetLabel",
                "selectionStart",
                "selectionEnd",
                "value",
                "menuPath",
                "reason",
                "sensitivity",
            ],
            &[
                "Click a visible button after observing the desktop.",
                "Cancel the current desktop action and release the controller lock.",
            ],
            "desktop_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_DESKTOP_STREAM,
            "desktop_stream",
            "Manage Computer Use desktop streaming state, WebRTC capability negotiation/signaling, and degraded screenshot fallback metadata.",
            &[
                "desktop",
                "computer_use",
                "stream",
                "webrtc",
                "screenshot_fallback",
                "manual_control",
            ],
            &[
                "action",
                "sessionId",
                "runId",
                "displayId",
                "streamId",
                "maxWidth",
                "maxFrameRate",
                "includeCursor",
                "quality",
                "iceServers",
                "sessionDescription",
                "iceCandidate",
            ],
            &[
                "Check stream capability before opening the cloud desktop viewport.",
                "Start a degraded screenshot-fallback stream.",
            ],
            "desktop_stream",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_WEB_SEARCH,
            "web",
            "Search the web through the configured backend for source discovery. Fetch top official/primary results before relying on their contents.",
            &["web", "search", "internet", "docs", "latest"],
            &["query", "resultCount", "timeoutMs"],
            &[
                "Search current official documentation.",
                "Find candidate sources, then fetch the primary pages.",
            ],
            "network",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_WEB_FETCH,
            "web",
            "Fetch HTTP or HTTPS text content from a selected result URL.",
            &["web", "fetch", "http", "docs", "page"],
            &["url", "maxChars", "timeoutMs"],
            &[
                "Fetch a documentation page after search.",
                "Inspect an official or primary source before answering.",
            ],
            "network",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            "browser_observe",
            "Observe the Browser Automation Service with capabilities, page text/source, snapshots/versioned refs, waits/assertions, screenshots, console, network, accessibility, forms, frames, timeline, and safe state reads.",
            &[
                "browser",
                "frontend",
                "ui",
                "dom",
                "snapshot",
                "refs",
                "assert",
                "screenshot",
                "accessibility",
                "console",
                "network",
                "storage",
                "cookies",
            ],
            &[
                "action",
                "url",
                "selector",
                "refId",
                "text",
                "condition",
                "assertion",
                "timeoutMs",
                "tabId",
                "area",
                "key",
            ],
            &[
                "Capture a snapshot and use refs for later actions.",
                "Assert page state and collect diagnostics.",
            ],
            "browser_observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            "browser_control",
            "Control the Browser Automation Service with navigation, selector/ref actions, semantic actions, form fill, batch execution, cookie/storage writes, evidence export, annotations, recordings, and tab actions.",
            &[
                "browser",
                "frontend",
                "ui",
                "dom",
                "click",
                "type",
                "ref",
                "batch",
                "form",
                "evidence",
                "navigation",
                "storage",
                "cookies",
            ],
            &[
                "action",
                "url",
                "selector",
                "refId",
                "text",
                "intent",
                "steps",
                "fields",
                "timeoutMs",
                "tabId",
                "area",
                "key",
            ],
            &["Run a ref-based browser batch after a snapshot."],
            "browser_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_EMULATOR,
            "emulator",
            "Drive mobile emulator automation.",
            &[
                "emulator",
                "mobile",
                "android",
                "ios",
                "device",
                "tap",
                "swipe",
                "screenshot",
            ],
            &["action", "input"],
            &["Launch an app and capture a screenshot."],
            "device_control",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_MCP_LIST,
            "mcp_list",
            "List connected MCP servers, tools, resources, and prompts over stdio, HTTP, or SSE without invoking capabilities.",
            &[
                "mcp",
                "model_context_protocol",
                "tool",
                "resource",
                "prompt",
                "external",
            ],
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
            &[
                "agent", "subagent", "delegate", "explore", "verify", "parallel",
            ],
            &[
                "action",
                "taskId",
                "role",
                "prompt",
                "modelId",
                "writeSet",
                "workflowStructure",
                "decision",
                "maxToolCalls",
            ],
            &[
                "Spawn a researcher for a bounded codebase question.",
                "Poll an engineer and integrate its result with a parent decision.",
            ],
            "agent_delegation",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_NOTEBOOK_EDIT,
            "notebook",
            "Edit a Jupyter notebook cell source with expected-hash protection.",
            &["notebook", "jupyter", "ipynb", "cell", "edit"],
            &[
                "path",
                "cellIndex",
                "expectedHash",
                "expectedSource",
                "replacementSource",
            ],
            &["Replace a notebook cell after reading it."],
            "write",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_CODE_INTEL,
            "intelligence",
            "Inspect source symbols or JSON diagnostics without requiring command execution.",
            &["code", "symbol", "diagnostic", "intelligence", "static"],
            &["action", "query", "path", "limit"],
            &[
                "Find symbols named greet.",
                "Read diagnostics for a source file.",
            ],
            "observe",
        ),
        catalog_entry(
            AUTONOMOUS_TOOL_LSP,
            "intelligence",
            "Inspect language-server availability and resolve source symbols or diagnostics through LSP with native fallback.",
            &[
                "lsp",
                "language_server",
                "symbol",
                "diagnostic",
                "install_suggestion",
            ],
            &["action", "query", "path", "limit", "serverId", "timeoutMs"],
            &[
                "List available language servers.",
                "Resolve symbol references with LSP.",
            ],
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

fn computer_use_default_tool_names() -> BTreeSet<&'static str> {
    [
        AUTONOMOUS_TOOL_READ,
        AUTONOMOUS_TOOL_READ_MANY,
        AUTONOMOUS_TOOL_RESULT_PAGE,
        AUTONOMOUS_TOOL_STAT,
        AUTONOMOUS_TOOL_SEARCH,
        AUTONOMOUS_TOOL_FIND,
        AUTONOMOUS_TOOL_GIT_STATUS,
        AUTONOMOUS_TOOL_GIT_DIFF,
        AUTONOMOUS_TOOL_LIST_TREE,
        AUTONOMOUS_TOOL_TOOL_ACCESS,
        AUTONOMOUS_TOOL_TOOL_SEARCH,
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
        AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
        AUTONOMOUS_TOOL_WORKSPACE_INDEX,
        AUTONOMOUS_TOOL_LIST,
        AUTONOMOUS_TOOL_DIRECTORY_DIGEST,
        AUTONOMOUS_TOOL_HASH,
        AUTONOMOUS_TOOL_TODO,
        AUTONOMOUS_TOOL_REQUEST_SENSITIVE_INPUT,
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
        AUTONOMOUS_TOOL_DESKTOP_CONTROL,
        AUTONOMOUS_TOOL_DESKTOP_STREAM,
        AUTONOMOUS_TOOL_EMULATOR,
        AUTONOMOUS_TOOL_MACOS_AUTOMATION,
        AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
    ]
    .into_iter()
    .collect()
}

fn computer_use_manifest_actions_for_tool(tool_name: &str) -> Vec<&'static str> {
    match tool_name {
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE => vec![
            "permissions_status",
            "display_list",
            "display_arrangement",
            "window_list",
            "app_list",
            "app_inventory",
            "notification_snapshot",
            "foreground_state",
            "screenshot",
            "cursor_state",
            "accessibility_snapshot",
            "ocr_snapshot",
            "element_at_point",
            "clipboard_read_text",
            "clipboard_read_html",
            "clipboard_read_rtf",
            "clipboard_read_image",
            "clipboard_read_files",
            "bridge_affordances",
            "health",
        ],
        AUTONOMOUS_TOOL_DESKTOP_CONTROL => vec![
            "mouse_down",
            "mouse_move",
            "mouse_click",
            "mouse_double_click",
            "mouse_right_click",
            "mouse_drag",
            "mouse_drag_move",
            "mouse_up",
            "scroll",
            "key_press",
            "hotkey",
            "volume_up",
            "volume_down",
            "volume_mute",
            "media_play_pause",
            "media_next_track",
            "media_prev_track",
            "type_text",
            "paste_text",
            "clipboard_write_text",
            "clipboard_write_html",
            "clipboard_write_rtf",
            "clipboard_write_image",
            "clipboard_write_files",
            "file_drop",
            "focus_window",
            "window_maximize",
            "window_minimize",
            "window_restore",
            "window_move_resize",
            "window_close",
            "activate_app",
            "launch_app",
            "quit_app",
            "ax_press",
            "ax_set_value",
            "ax_focus",
            "ax_select",
            "ax_confirm",
            "ax_cancel",
            "ax_increment",
            "ax_decrement",
            "ax_expand",
            "ax_collapse",
            "ax_scroll_to_visible",
            "ax_toggle",
            "menu_select",
            "dock_item_press",
            "status_item_press",
            "file_dialog_set_path",
            "file_dialog_confirm",
            "cancel_current_action",
        ],
        AUTONOMOUS_TOOL_DESKTOP_STREAM => vec![
            "stream_capabilities",
            "stream_start",
            "stream_offer",
            "stream_answer",
            "stream_ice_candidate",
            "stream_stop",
            "stream_status",
            "stream_set_quality",
            "stream_request_keyframe",
        ],
        AUTONOMOUS_TOOL_MACOS_AUTOMATION => vec![
            "mac_permissions",
            "mac_app_list",
            "mac_app_launch",
            "mac_app_activate",
            "mac_app_quit",
            "mac_window_list",
            "mac_window_focus",
            "mac_screenshot",
        ],
        AUTONOMOUS_TOOL_HOST_COMMAND => vec!["preview", "run"],
        _ => Vec::new(),
    }
}

fn computer_use_manifest_rollout_gate(tool_name: &str) -> JsonValue {
    match tool_name {
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE
        | AUTONOMOUS_TOOL_DESKTOP_CONTROL
        | AUTONOMOUS_TOOL_DESKTOP_STREAM => json!({
            "controlledBy": [
                DESKTOP_FEATURE_MASTER_ENV,
                desktop_tool_feature_env(tool_name).unwrap_or(DESKTOP_FEATURE_MASTER_ENV),
                DESKTOP_FEATURE_ROLLOUT_PERCENT_ENV,
                DESKTOP_FEATURE_ROLLOUT_ID_ENV
            ],
            "defaultEnabled": desktop_tool_default_enabled(),
            "currentlyEnabled": desktop_tool_available_by_rollout(tool_name),
        }),
        _ => json!({
            "controlledBy": [],
            "defaultEnabled": true,
            "currentlyEnabled": true,
        }),
    }
}

fn computer_use_manifest_platform_gate(tool_name: &str) -> JsonValue {
    match tool_name {
        AUTONOMOUS_TOOL_MACOS_AUTOMATION | AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED => json!({
            "supportedPlatforms": ["macos"],
            "currentPlatformSupported": cfg!(target_os = "macos"),
        }),
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE
        | AUTONOMOUS_TOOL_DESKTOP_CONTROL
        | AUTONOMOUS_TOOL_DESKTOP_STREAM => json!({
            "supportedPlatforms": ["macos", "windows", "linux"],
            "currentPlatformSupported": cfg!(any(target_os = "macos", target_os = "windows", target_os = "linux")),
        }),
        AUTONOMOUS_TOOL_POWERSHELL => json!({
            "supportedPlatforms": ["windows"],
            "currentPlatformSupported": cfg!(target_os = "windows"),
        }),
        AUTONOMOUS_TOOL_HOST_COMMAND => json!({
            "supportedPlatforms": ["macos", "windows", "linux"],
            "currentPlatformSupported": cfg!(any(target_os = "macos", target_os = "windows", target_os = "linux")),
        }),
        _ => json!({
            "supportedPlatforms": ["macos", "windows", "linux"],
            "currentPlatformSupported": true,
        }),
    }
}

fn computer_use_manifest_approval_requirement(tool_name: &str) -> &'static str {
    match tool_name {
        AUTONOMOUS_TOOL_DESKTOP_CONTROL => {
            "desktop policy gate; app quit and high-risk text/actions require operator approval or denial"
        }
        AUTONOMOUS_TOOL_DESKTOP_STREAM => {
            "desktop policy gate; stream start/stop and signaling are audited"
        }
        AUTONOMOUS_TOOL_MACOS_AUTOMATION => {
            "macOS app quit requires operator approval; observation and app/window focus are bounded"
        }
        AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED => {
            "operator approval required for privileged diagnostics"
        }
        AUTONOMOUS_TOOL_COMMAND_RUN
        | AUTONOMOUS_TOOL_COMMAND_SESSION
        | AUTONOMOUS_TOOL_POWERSHELL => {
            "repo-scoped command policy; unsafe or mutating commands require approval"
        }
        AUTONOMOUS_TOOL_HOST_COMMAND => {
            "Owner Admin mode must be locally enabled and unexpired; non-preview host commands require per-command operator approval"
        }
        AUTONOMOUS_TOOL_PROCESS_MANAGER => {
            "process-control policy; external or destructive process actions require approval"
        }
        _ => "tool-specific safety policy",
    }
}

fn computer_use_manifest_permission_gate(tool_name: &str) -> JsonValue {
    match tool_name {
        AUTONOMOUS_TOOL_DESKTOP_OBSERVE
        | AUTONOMOUS_TOOL_DESKTOP_CONTROL
        | AUTONOMOUS_TOOL_DESKTOP_STREAM => json!({
            "state": "checked_by_desktop_runtime",
            "missingReasonCode": "permission_denied",
            "statusSource": "desktop_observe.permissions_status",
            "requiredPermissions": [
                "macos_screen_recording",
                "macos_accessibility",
                "macos_input_monitoring",
                "windows_active_user_session",
                "windows_notification_listener"
            ],
            "userAction": "Open desktop_observe with action=permissions_status for current OS permission state and remediation."
        }),
        AUTONOMOUS_TOOL_MACOS_AUTOMATION => json!({
            "state": "checked_by_macos_automation_runtime",
            "missingReasonCode": "permission_denied",
            "statusSource": "macos_automation.mac_permissions",
            "requiredPermissions": [
                "macos_screen_recording",
                "macos_accessibility"
            ],
            "userAction": "Open macos_automation with action=mac_permissions for current TCC state and remediation."
        }),
        AUTONOMOUS_TOOL_HOST_COMMAND => json!({
            "state": "owner_admin_mode_required",
            "missingReasonCode": "owner_admin_mode_inactive_or_expired",
            "statusSource": "desktop_control_settings.policyProfile and ownerAdminExpiresAt",
            "requiredPermissions": [
                "local_owner_admin_mode",
                "operator_approval_for_run",
                "os_native_elevation_prompt_when_required"
            ],
            "userAction": "Enable local Owner Admin mode in desktop-control settings, preview high-impact commands, then approve the exact run."
        }),
        AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED => json!({
            "state": "operator_approval_required",
            "missingReasonCode": "privileged_diagnostics_approval_required",
            "statusSource": "tool policy decision",
            "requiredPermissions": ["operator_approval"],
            "userAction": "Request approval before running privileged diagnostics."
        }),
        _ => json!({
            "state": "not_applicable",
            "missingReasonCode": null,
            "statusSource": null,
            "requiredPermissions": [],
            "userAction": null
        }),
    }
}

fn computer_use_manifest_runtime_gate(
    tool_name: &str,
    runtime_available: bool,
    skill_tool_enabled: bool,
) -> JsonValue {
    let pack_ids = domain_tool_pack_ids_for_tool(tool_name);
    let missing_reason = if runtime_available {
        None
    } else if tool_name == AUTONOMOUS_TOOL_SKILL && !skill_tool_enabled {
        Some("skill_runtime_not_enabled")
    } else if pack_ids.iter().any(|pack_id| pack_id == "browser") {
        Some("browser_executor_not_installed")
    } else if pack_ids.iter().any(|pack_id| pack_id == "emulator") {
        Some("emulator_executor_not_installed")
    } else if pack_ids.iter().any(|pack_id| pack_id == "solana") {
        Some("solana_executor_not_installed")
    } else {
        Some("runtime_executor_or_installation_unavailable")
    };

    json!({
        "available": runtime_available,
        "missingReasonCode": missing_reason,
        "toolPackIds": pack_ids,
        "statusSource": "AutonomousToolRuntime::tool_available_by_runtime"
    })
}

fn computer_use_manifest_activation_gate(
    active_by_default: bool,
    available_through_tool_access: bool,
    activation_groups: &[String],
) -> JsonValue {
    let state = if active_by_default {
        "active_by_default_when_selected_by_planner"
    } else if available_through_tool_access {
        "requestable_through_tool_access"
    } else {
        "not_active_or_not_requestable"
    };
    let missing_reason = if active_by_default || available_through_tool_access {
        None
    } else {
        Some("not_activated_or_not_requestable")
    };

    json!({
        "state": state,
        "missingReasonCode": missing_reason,
        "activationGroups": activation_groups,
        "statusSource": "tool_access.available_groups and ToolRegistrySnapshot.exposurePlan"
    })
}

fn computer_use_manifest_provider_gate(
    active_by_default: bool,
    available_through_tool_access: bool,
) -> JsonValue {
    let eligible = active_by_default || available_through_tool_access;
    json!({
        "eligibleForProviderProjection": eligible,
        "missingReasonCode": if eligible { JsonValue::Null } else { json!("provider_limit_or_projection_filter") },
        "statusSource": "tool_access.exposure_diagnostics.traceLocation",
        "debugSurface": "xero.tool_exposure_diagnostics.v1"
    })
}

fn computer_use_manifest_availability_reasons(
    allowed_by_policy: bool,
    host_available: bool,
    runtime_available: bool,
    active_by_default: bool,
    available_through_tool_access: bool,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if !allowed_by_policy {
        reasons.push("blocked_by_computer_use_policy");
    }
    if !host_available {
        reasons.push("blocked_by_platform_or_rollout");
    }
    if host_available && !runtime_available {
        reasons.push("runtime_executor_or_installation_unavailable");
    }
    if active_by_default {
        reasons.push("active_by_default_when_selected_by_planner");
    }
    if available_through_tool_access {
        reasons.push("requestable_through_tool_access");
    }
    if reasons.is_empty() {
        reasons.push("not_active_or_not_requestable_in_current_runtime");
    }
    reasons
}

struct ComputerUseManifestAvailabilityContext<'a> {
    tool_name: &'a str,
    allowed_by_policy: bool,
    host_available: bool,
    runtime_available: bool,
    active_by_default: bool,
    available_through_tool_access: bool,
    activation_groups: &'a [String],
    skill_tool_enabled: bool,
}

fn computer_use_manifest_availability_diagnostics(
    context: ComputerUseManifestAvailabilityContext<'_>,
) -> JsonValue {
    let ComputerUseManifestAvailabilityContext {
        tool_name,
        allowed_by_policy,
        host_available,
        runtime_available,
        active_by_default,
        available_through_tool_access,
        activation_groups,
        skill_tool_enabled,
    } = context;
    let rollout_gate = computer_use_manifest_rollout_gate(tool_name);
    let platform_gate = computer_use_manifest_platform_gate(tool_name);
    let availability_reasons = computer_use_manifest_availability_reasons(
        allowed_by_policy,
        host_available,
        runtime_available,
        active_by_default,
        available_through_tool_access,
    );

    json!({
        "schema": "xero.computer_use_tool_availability_diagnostics.v1",
        "policy": {
            "allowed": allowed_by_policy,
            "missingReasonCode": if allowed_by_policy { JsonValue::Null } else { json!("blocked_by_computer_use_policy") },
            "statusSource": "runtime agent policy"
        },
        "providerProjection": computer_use_manifest_provider_gate(active_by_default, available_through_tool_access),
        "rollout": {
            "currentlyEnabled": rollout_gate["currentlyEnabled"].clone(),
            "controlledBy": rollout_gate["controlledBy"].clone(),
            "missingReasonCode": if rollout_gate["currentlyEnabled"].as_bool().unwrap_or(true) { JsonValue::Null } else { json!("blocked_by_rollout_flag") },
            "statusSource": "desktop rollout environment"
        },
        "platform": {
            "currentPlatformSupported": platform_gate["currentPlatformSupported"].clone(),
            "supportedPlatforms": platform_gate["supportedPlatforms"].clone(),
            "missingReasonCode": if platform_gate["currentPlatformSupported"].as_bool().unwrap_or(true) { JsonValue::Null } else { json!("platform_unsupported") },
            "statusSource": "compile-time target cfg"
        },
        "permission": computer_use_manifest_permission_gate(tool_name),
        "runtime": computer_use_manifest_runtime_gate(tool_name, runtime_available, skill_tool_enabled),
        "activation": computer_use_manifest_activation_gate(active_by_default, available_through_tool_access, activation_groups),
        "reasonCodes": availability_reasons
    })
}

fn workstation_control_pack_status(runtime: &AutonomousToolRuntime) -> JsonValue {
    json!({
        "desktopSidecar": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_DESKTOP_OBSERVE)
                || runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_DESKTOP_CONTROL)
                || runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_DESKTOP_STREAM),
            "tools": [
                AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
                AUTONOMOUS_TOOL_DESKTOP_CONTROL,
                AUTONOMOUS_TOOL_DESKTOP_STREAM
            ],
        },
        "browserControl": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_BROWSER_OBSERVE)
                || runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_BROWSER_CONTROL),
            "tools": [
                AUTONOMOUS_TOOL_BROWSER_OBSERVE,
                AUTONOMOUS_TOOL_BROWSER_CONTROL
            ],
        },
        "hostCommandAdmin": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_COMMAND_RUN)
                || runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_COMMAND_SESSION)
                || runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_HOST_COMMAND)
                || runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_POWERSHELL),
            "tools": [
                AUTONOMOUS_TOOL_COMMAND_RUN,
                AUTONOMOUS_TOOL_COMMAND_SESSION,
                AUTONOMOUS_TOOL_HOST_COMMAND,
                AUTONOMOUS_TOOL_POWERSHELL
            ],
            "mode": "host_command_requires_unexpired_owner_admin_mode",
        },
        "clipboardFileDrop": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_DESKTOP_CONTROL),
            "tools": [AUTONOMOUS_TOOL_DESKTOP_CONTROL],
            "mode": "clipboard_read_text/html/rtf/image/files require operator approval; clipboard_write_text/html/rtf/image/files, paste_text, and file_drop are audited desktop_control actions; uncommon rich clipboard formats beyond HTML and RTF remain future work",
        },
        "browserTerminalBridge": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_DESKTOP_OBSERVE),
            "tools": [
                AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
                AUTONOMOUS_TOOL_BROWSER_OBSERVE,
                AUTONOMOUS_TOOL_BROWSER_CONTROL,
                AUTONOMOUS_TOOL_COMMAND_RUN,
                AUTONOMOUS_TOOL_COMMAND_SESSION,
                AUTONOMOUS_TOOL_HOST_COMMAND
            ],
            "mode": "desktop_observe.bridge_affordances reports when focused desktop state should hand off to browser or command tools instead of pixel input",
        },
        "appInventory": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_DESKTOP_OBSERVE),
            "tools": [AUTONOMOUS_TOOL_DESKTOP_OBSERVE],
            "mode": "desktop_observe.app_inventory reports installed launch targets and merges running window state when platform APIs permit it",
        },
        "notificationObservation": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_DESKTOP_OBSERVE),
            "tools": [AUTONOMOUS_TOOL_DESKTOP_OBSERVE],
            "mode": "desktop_observe.notification_snapshot is approval-gated; Windows uses the UserNotificationListener when OS permission is granted, while macOS reports platform-policy diagnostics",
        },
        "ocrUiTree": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_DESKTOP_OBSERVE),
            "tools": [AUTONOMOUS_TOOL_DESKTOP_OBSERVE],
            "platformNotes": "macOS exposes Accessibility and Vision OCR; Windows exposes UI Automation and Windows.Media.Ocr through the desktop sidecar when the active user profile has an OCR engine.",
        },
        "diagnostics": {
            "available": runtime.tool_available_by_runtime(AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE),
            "tools": [
                AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
                AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_PRIVILEGED
            ],
        },
    })
}

fn workstation_capability_report_fixtures() -> JsonValue {
    json!([
        {
            "platform": "macos",
            "capabilities": [
                "screen_capture",
                "accessibility_tree",
                "vision_ocr",
                "pointer_keyboard_text",
                "clipboard_text_html_rtf_image_files",
                "file_drop_handoff",
                "app_inventory_launch_targets",
                "notification_observation_diagnostics",
                "app_window_control",
                "native_webrtc_stream",
                "volume_media_keys"
            ],
            "permissionStates": [
                "screen_recording_granted_or_denied",
                "accessibility_granted_or_denied",
                "input_monitoring_granted_or_denied"
            ],
            "verificationScenarios": [
                {
                    "id": "macos_screen_recording_denied",
                    "category": "permission",
                    "evidence": "permissions_status row: Screen Recording=denied; screenshot/stream actions return user-actionable permission diagnostics"
                },
                {
                    "id": "macos_screen_recording_granted",
                    "category": "permission",
                    "evidence": "permissions_status row: Screen Recording=granted; screenshot and native WebRTC stream start can capture a display"
                },
                {
                    "id": "macos_accessibility_denied",
                    "category": "permission",
                    "evidence": "permissions_status row: Accessibility=denied; AX actions fail with permission_accessibility_denied"
                },
                {
                    "id": "macos_accessibility_granted",
                    "category": "permission",
                    "evidence": "permissions_status row: Accessibility=granted; accessibility_snapshot, element_at_point, and AX actions return structured results"
                },
                {
                    "id": "macos_input_monitoring_denied",
                    "category": "permission",
                    "evidence": "permissions_status row: Input Monitoring=denied or backend diagnostic; keyboard/pointer actions return user-actionable diagnostics"
                },
                {
                    "id": "macos_input_monitoring_granted",
                    "category": "permission",
                    "evidence": "permissions_status row: Input Monitoring=granted or not required; pointer, keyboard, paste, and media-key actions execute through the sidecar"
                },
                {
                    "id": "macos_multiple_displays",
                    "category": "hardware",
                    "evidence": "display_arrangement reports multiple display rows, virtual bounds, primary display, scale factors, gaps, and overlaps"
                },
                {
                    "id": "macos_retina_scaling",
                    "category": "hardware",
                    "evidence": "display_list/display_arrangement include scaleFactor > 1 and screenshot dimensions match captured pixels"
                },
                {
                    "id": "macos_secure_input",
                    "category": "os_boundary",
                    "evidence": "keyboard/AX failure diagnostics identify secure input or protected surfaces without attempting a bypass"
                }
            ],
            "knownUnavailable": [
                "tcc_sip_secure_input_bypass",
                "credential_prompt_bypass",
                "uncommon_rich_clipboard_formats",
                "notification_center_observation",
                "spaces_mission_control_structured_automation"
            ]
        },
        {
            "platform": "windows",
            "capabilities": [
                "screen_capture",
                "uia_tree",
                "windows_media_ocr",
                "pointer_keyboard_text",
                "clipboard_text_html_rtf_image_files",
                "file_drop_handoff",
                "app_inventory_launch_targets",
                "notification_observation_with_user_listener_permission",
                "app_window_control",
                "native_webrtc_stream",
                "dxgi_output_duplication_capture",
                "openh264_software_encoding",
                "best_effort_cursor_overlay",
                "screenshot_fallback_stream",
                "volume_media_keys"
            ],
            "permissionStates": [
                "screen_capture_active_user_session",
                "desktop_input_active_user_session",
                "uia_active_user_session",
                "notification_listener_allowed_or_diagnostic",
                "ocr_engine_available_or_diagnostic"
            ],
            "verificationScenarios": [
                {
                    "id": "windows_standard_user",
                    "category": "session",
                    "evidence": "permissions_status reports active-user-session capture/input/UIA; non-elevated apps support screenshots, OCR, UIA, pointer, keyboard, clipboard, and WebRTC stream"
                },
                {
                    "id": "windows_administrator",
                    "category": "session",
                    "evidence": "owner-admin mode and host_command remain approval-gated; elevated OS prompts are surfaced to the local user rather than bypassed"
                },
                {
                    "id": "windows_uac_prompt",
                    "category": "os_boundary",
                    "evidence": "secure desktop/UAC surfaces are reported as unavailable for automation; host_command asks the user to approve OS-native elevation prompts"
                },
                {
                    "id": "windows_multiple_dpi_scales",
                    "category": "hardware",
                    "evidence": "display_list/display_arrangement report per-display scale factors; desktop_control maps sourceWidth/sourceHeight into native coordinates"
                },
                {
                    "id": "windows_multiple_monitors",
                    "category": "hardware",
                    "evidence": "display_arrangement reports virtual bounds, primary display, gaps, overlaps, and WebRTC capture target selection by displayId"
                },
                {
                    "id": "windows_rdp_session",
                    "category": "session",
                    "evidence": "capability report records active session capture/UIA/OCR availability or user-actionable unavailable diagnostics"
                },
                {
                    "id": "windows_store_app",
                    "category": "application",
                    "evidence": "app_inventory includes AppsFolder launch targets; UIA snapshot/control returns structured rows or actionable unsupported diagnostics"
                },
                {
                    "id": "windows_win32_app",
                    "category": "application",
                    "evidence": "window_list/focus/app_control work with HWND-backed windows; UIA Invoke/Value/Focus patterns operate where providers expose them"
                },
                {
                    "id": "windows_electron_app",
                    "category": "application",
                    "evidence": "UIA snapshot/control and OCR work for Chromium/Electron controls, with browser bridge affordances when a precise browser tool is better"
                },
                {
                    "id": "windows_browser_app",
                    "category": "application",
                    "evidence": "bridge_affordances prefers browser_observe/browser_control over pixel input for browser surfaces"
                },
                {
                    "id": "windows_explorer_app",
                    "category": "application",
                    "evidence": "window/app inventory, UIA menus/items, file clipboard, file_drop, and dialogs helpers support Explorer-style workflows"
                },
                {
                    "id": "windows_office_style_app",
                    "category": "application",
                    "evidence": "UIA snapshot/control, OCR, rich clipboard HTML/RTF, and file dialogs expose Office-style app workflows with unsupported-control diagnostics"
                }
            ],
            "knownUnavailable": [
                "uac_secure_desktop_bypass",
                "credential_provider_bypass",
                "uncommon_rich_clipboard_formats"
            ]
        },
        {
            "platform": "cross_platform_failure_modes",
            "capabilities": [
                "failure_diagnostics",
                "approval_audit",
                "emergency_stop"
            ],
            "permissionStates": [],
            "verificationScenarios": [
                {
                    "id": "sidecar_unavailable",
                    "category": "failure_mode",
                    "evidence": "desktop status and tool calls return sidecar health/error rows when the sidecar cannot start or authenticate"
                },
                {
                    "id": "sidecar_operation_unimplemented",
                    "category": "failure_mode",
                    "evidence": "unimplemented sidecar operations return sidecar_operation_unimplemented without silent coordinate fallback"
                },
                {
                    "id": "screenshot_capture_denied",
                    "category": "failure_mode",
                    "evidence": "screenshot/capture APIs return permission diagnostics and preserve independent input-command budgeting"
                },
                {
                    "id": "uia_unavailable",
                    "category": "failure_mode",
                    "evidence": "Windows UIA failures return desktop_windows_uia_* diagnostics and keep pointer/keyboard fallback explicit"
                },
                {
                    "id": "ocr_unavailable",
                    "category": "failure_mode",
                    "evidence": "OCR snapshots return performed=false or desktop_windows_ocr_unavailable diagnostics when language engines are missing"
                },
                {
                    "id": "stream_start_failure",
                    "category": "failure_mode",
                    "evidence": "stream_start failures populate stream status/fallbackReason without leaking frame bytes into audit logs"
                },
                {
                    "id": "local_user_takeover",
                    "category": "failure_mode",
                    "evidence": "controller lock and emergency-stop tests prove local-user takeover blocks remote reacquire"
                }
            ],
            "knownUnavailable": [
                "os_security_boundary_bypass",
                "credential_prompt_bypass"
            ]
        }
    ])
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
    pub(super) linked_read_roots: Vec<AutonomousLinkedReadRoot>,
    pub(super) context_access_ledger: Arc<Mutex<ContextAccessLedger>>,
    pub(super) tool_application_policy: ResolvedAgentToolApplicationStyleDto,
    pub(super) browser_control_preference: BrowserControlPreferenceDto,
    pub(super) soul_settings: SoulSettingsDto,
    pub(super) browser_executor: Option<Arc<dyn BrowserExecutor>>,
    pub(super) emulator_executor: Option<Arc<dyn EmulatorExecutor>>,
    pub(super) solana_executor: Option<Arc<dyn SolanaExecutor>>,
    pub(super) desktop_control: desktop_control::DesktopControlState,
    pub(super) cancellation_token: Option<AgentRunCancellationToken>,
    pub(super) tool_execution_cancelled: Option<Arc<dyn Fn() -> bool + Send + Sync>>,
    pub(super) mcp_registry_path: Option<PathBuf>,
    pub(super) environment_profile_database_path: Option<PathBuf>,
    pub(super) todo_items: Arc<Mutex<BTreeMap<String, AutonomousTodoItem>>>,
    pub(super) subagent_tasks: Arc<Mutex<BTreeMap<String, AutonomousSubagentTask>>>,
    pub(super) subagent_executor: Option<Arc<dyn AutonomousSubagentExecutor>>,
    pub(super) subagent_execution_depth: usize,
    pub(super) subagent_child_identity: Option<AutonomousSubagentChildIdentity>,
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
            .field("linked_read_roots", &self.linked_read_roots)
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
            .field(
                "subagent_child_identity_bound",
                &self.subagent_child_identity.is_some(),
            )
            .field("subagent_write_scope", &self.subagent_write_scope)
            .field("subagent_limits", &self.subagent_limits)
            .field("delegated_usage_budget", &self.delegated_usage_budget)
            .field("skill_tool_enabled", &self.skill_tool.is_some())
            .field("desktop_control", &"enabled")
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
pub(super) struct AutonomousLinkedReadRoot {
    pub(super) path: PathBuf,
    pub(super) is_dir: bool,
}

#[derive(Debug, Clone, Default)]
pub(super) struct ContextAccessLedger {
    pub project_context_searches: BTreeMap<String, AutonomousProjectContextOutput>,
    pub manifest_inspections: BTreeMap<String, AutonomousProjectContextOutput>,
    pub workspace_index_statuses: BTreeMap<String, AutonomousWorkspaceIndexOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutonomousSubagentWriteScope {
    pub role: AutonomousSubagentRole,
    pub write_set: Vec<String>,
}

pub const AUTONOMOUS_SUBAGENT_CHILD_IDENTITY_SCHEMA: &str =
    "xero.autonomous_subagent_child_identity.v1";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentInheritedContext {
    pub kind: String,
    pub source_run_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentChildIdentity {
    pub schema: String,
    pub role: AutonomousSubagentRole,
    pub role_label: String,
    pub verification_contract: String,
    pub initial_prompt: String,
    pub parent_run_id: String,
    pub parent_trace_id: String,
    pub parent_subagent_id: String,
    pub depth: usize,
    pub definition_snapshot: JsonValue,
    pub inherited_context: Vec<AutonomousSubagentInheritedContext>,
}

impl AutonomousSubagentChildIdentity {
    pub(crate) fn validate_for_run(&self, run_prompt: &str) -> CommandResult<()> {
        if self.schema != AUTONOMOUS_SUBAGENT_CHILD_IDENTITY_SCHEMA {
            return Err(CommandError::system_fault(
                "agent_subagent_child_identity_schema_invalid",
                format!(
                    "Xero cannot use child identity schema `{}` because it is unsupported.",
                    self.schema
                ),
            ));
        }
        if self.initial_prompt != run_prompt {
            return Err(CommandError::system_fault(
                "agent_subagent_child_identity_prompt_mismatch",
                "Xero cannot use a child identity whose initial prompt disagrees with the run prompt.",
            ));
        }

        let definition_identity = self
            .definition_snapshot
            .get("subagentIdentity")
            .and_then(JsonValue::as_object);
        let definition_inherited_context = definition_identity
            .and_then(|identity| identity.get("inheritedContext"))
            .cloned()
            .and_then(|value| {
                serde_json::from_value::<Vec<AutonomousSubagentInheritedContext>>(value).ok()
            });
        let definition_depth = definition_identity
            .and_then(|identity| identity.get("depth"))
            .and_then(JsonValue::as_u64)
            .and_then(|depth| usize::try_from(depth).ok());
        let inherited_context_is_valid = !self.inherited_context.is_empty()
            && self.inherited_context.iter().all(|context| {
                !context.kind.trim().is_empty()
                    && context.source_run_id == self.parent_run_id
                    && !context.reason.trim().is_empty()
            });
        let child_tool_policy =
            AutonomousAgentToolPolicy::from_definition_snapshot(&self.definition_snapshot);
        let role_policy = AutonomousAgentToolPolicy::from_subagent_role(self.role);
        let child_tool_policy_is_bounded = child_tool_policy.as_ref().is_some_and(|policy| {
            policy.allowed_effect_classes.is_empty()
                && policy.allowed_tool_packs.is_empty()
                && policy.allowed_mcp_servers.is_empty()
                && policy.allowed_dynamic_tools.is_empty()
                && !policy.subagent_allowed
                && policy.allowed_subagent_roles.is_empty()
                && policy
                    .allowed_tools
                    .iter()
                    .all(|tool| role_policy.allows_tool(tool))
        });
        let definition_matches = self
            .definition_snapshot
            .get("scope")
            .and_then(JsonValue::as_str)
            == Some("runtime_child")
            && self
                .definition_snapshot
                .get("id")
                .and_then(JsonValue::as_str)
                .is_some_and(|id| !id.trim().is_empty())
            && self
                .definition_snapshot
                .get("version")
                .and_then(JsonValue::as_u64)
                .is_some_and(|version| version > 0)
            && definition_identity
                .and_then(|identity| identity.get("schema"))
                .and_then(JsonValue::as_str)
                == Some(AUTONOMOUS_SUBAGENT_CHILD_IDENTITY_SCHEMA)
            && definition_identity
                .and_then(|identity| identity.get("role"))
                .and_then(JsonValue::as_str)
                == Some(self.role.as_str())
            && definition_identity
                .and_then(|identity| identity.get("roleLabel"))
                .and_then(JsonValue::as_str)
                == Some(self.role_label.as_str())
            && definition_identity
                .and_then(|identity| identity.get("parentRunId"))
                .and_then(JsonValue::as_str)
                == Some(self.parent_run_id.as_str())
            && definition_identity
                .and_then(|identity| identity.get("parentTraceId"))
                .and_then(JsonValue::as_str)
                == Some(self.parent_trace_id.as_str())
            && definition_identity
                .and_then(|identity| identity.get("parentSubagentId"))
                .and_then(JsonValue::as_str)
                == Some(self.parent_subagent_id.as_str())
            && definition_identity
                .and_then(|identity| identity.get("verificationContract"))
                .and_then(JsonValue::as_str)
                == Some(self.verification_contract.as_str())
            && definition_depth == Some(self.depth)
            && definition_inherited_context.as_ref() == Some(&self.inherited_context);
        let child_contract_is_valid = self.depth > 0
            && !self.role_label.trim().is_empty()
            && !self.verification_contract.trim().is_empty()
            && !self.parent_run_id.trim().is_empty()
            && !self.parent_trace_id.trim().is_empty()
            && !self.parent_subagent_id.trim().is_empty()
            && inherited_context_is_valid
            && definition_matches
            && child_tool_policy_is_bounded
            && self
                .definition_snapshot
                .get("promptFragments")
                .and_then(JsonValue::as_array)
                .is_some_and(Vec::is_empty)
            && self
                .definition_snapshot
                .get("finalResponseContract")
                .and_then(JsonValue::as_str)
                == Some(self.verification_contract.as_str())
            && self
                .definition_snapshot
                .get("attachedSkills")
                .and_then(JsonValue::as_array)
                .is_some_and(Vec::is_empty)
            && self
                .definition_snapshot
                .get("handoffPolicy")
                .and_then(|policy| policy.get("enabled"))
                .and_then(JsonValue::as_bool)
                == Some(false);
        if !child_contract_is_valid {
            return Err(CommandError::system_fault(
                "agent_subagent_child_identity_invalid",
                "Xero refused a child identity because its typed identity, definition, or inheritance metadata is incomplete or inconsistent.",
            ));
        }
        if let Some(workflow_structure) = self.definition_snapshot.get("workflowStructure") {
            let Some(child_tool_policy) = child_tool_policy.as_ref() else {
                return Err(CommandError::system_fault(
                    "agent_subagent_child_identity_invalid",
                    "Xero refused a child identity because its definition has no child tool policy.",
                ));
            };
            validate_subagent_workflow_structure(
                Some(workflow_structure.clone()),
                child_tool_policy,
            )
            .map_err(|error| {
                CommandError::system_fault(
                    "agent_subagent_child_stage_invalid",
                    format!(
                        "Xero refused a child identity because its Stage configuration is invalid: {}",
                        error.message
                    ),
                )
            })?;
        }
        Ok(())
    }
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

fn canonical_linked_read_roots<I>(roots: I) -> CommandResult<Vec<AutonomousLinkedReadRoot>>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut linked_roots = Vec::<AutonomousLinkedReadRoot>::new();
    for root in roots {
        let trimmed = root.to_string_lossy().trim().to_string();
        if trimmed.is_empty() {
            continue;
        }
        let canonical = fs::canonicalize(&root).map_err(|error| {
            CommandError::user_fixable(
                "linked_context_path_unavailable",
                format!(
                    "Xero could not access linked context path `{}`: {error}",
                    root.display()
                ),
            )
        })?;
        let metadata = fs::metadata(&canonical).map_err(|error| {
            CommandError::user_fixable(
                "linked_context_path_unavailable",
                format!(
                    "Xero could not inspect linked context path `{}`: {error}",
                    canonical.display()
                ),
            )
        })?;
        let is_dir = metadata.is_dir();
        if !is_dir && !metadata.is_file() {
            return Err(CommandError::user_fixable(
                "linked_context_path_unsupported",
                format!(
                    "Xero can only grant linked context read access for files and folders; `{}` is neither.",
                    canonical.display()
                ),
            ));
        }
        if linked_roots
            .iter()
            .any(|existing| existing.path == canonical && existing.is_dir == is_dir)
        {
            continue;
        }
        linked_roots.push(AutonomousLinkedReadRoot {
            path: canonical,
            is_dir,
        });
    }
    Ok(linked_roots)
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
            linked_read_roots: Vec::new(),
            context_access_ledger: Arc::new(Mutex::new(ContextAccessLedger::default())),
            tool_application_policy: ResolvedAgentToolApplicationStyleDto::default(),
            browser_control_preference: BrowserControlPreferenceDto::Default,
            soul_settings: default_soul_settings(),
            browser_executor: None,
            emulator_executor: None,
            solana_executor: None,
            desktop_control: desktop_control::DesktopControlState::default(),
            cancellation_token: None,
            tool_execution_cancelled: None,
            mcp_registry_path: None,
            environment_profile_database_path: None,
            todo_items: Arc::new(Mutex::new(BTreeMap::new())),
            subagent_tasks: Arc::new(Mutex::new(BTreeMap::new())),
            subagent_executor: None,
            subagent_execution_depth: 0,
            subagent_child_identity: None,
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
        Self::for_project_with_provider_config(app, state, project_id, None)
    }

    pub fn for_project_with_provider_config<R: Runtime>(
        app: &AppHandle<R>,
        state: &DesktopState,
        project_id: &str,
        provider_config: Option<&AgentProviderConfig>,
    ) -> CommandResult<Self> {
        let browser_executor = browser::tauri_browser_executor(app.clone(), state.clone());
        let repo_root =
            crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
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
            crate::commands::autonomous_web_search::resolve_autonomous_web_config(
                app,
                state,
                provider_config,
            )?,
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

    pub fn with_agent_workflow_replay(mut self, replay: AutonomousAgentWorkflowReplay) -> Self {
        self.todo_items = Arc::new(Mutex::new(replay.todo_items.clone()));
        let mut state = self
            .agent_workflow_policy
            .as_ref()
            .map(AutonomousAgentWorkflowPolicy::initial_state)
            .unwrap_or_default();
        state.tool_successes = replay.tool_successes;
        if let Some(policy) = self.agent_workflow_policy.as_ref() {
            policy.advance_state(&mut state, &replay.todo_items);
        }
        self.agent_workflow_state = Arc::new(Mutex::new(state));
        self
    }

    pub fn with_agent_run_context(
        mut self,
        project_id: impl Into<String>,
        agent_session_id: impl Into<String>,
        run_id: impl Into<String>,
    ) -> Self {
        let next_context = AutonomousAgentRunContext {
            project_id: project_id.into(),
            agent_session_id: agent_session_id.into(),
            run_id: run_id.into(),
        };
        let run_changed = match self.agent_run_context.as_ref() {
            Some(current) => current != &next_context,
            None => true,
        };
        if run_changed {
            self.context_access_ledger = Arc::new(Mutex::new(ContextAccessLedger::default()));
        }
        self.agent_run_context = Some(next_context);
        self
    }

    pub fn with_linked_read_roots<I>(mut self, roots: I) -> CommandResult<Self>
    where
        I: IntoIterator<Item = PathBuf>,
    {
        self.linked_read_roots = canonical_linked_read_roots(roots)?;
        Ok(self)
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

    pub(crate) fn with_subagent_child_identity(
        mut self,
        identity: AutonomousSubagentChildIdentity,
    ) -> Self {
        self.subagent_child_identity = Some(identity);
        self
    }

    pub fn subagent_child_identity(&self) -> Option<&AutonomousSubagentChildIdentity> {
        self.subagent_child_identity.as_ref()
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
        self.enforce_mailbox_check_before_mutation(tool_name, &request)?;
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
            AutonomousToolRequest::ReadMany(request) => self.read_many(request),
            AutonomousToolRequest::ResultPage(request) => self.result_page(request),
            AutonomousToolRequest::Stat(request) => self.stat(request),
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
            AutonomousToolRequest::Copy(request) => self.copy(request),
            AutonomousToolRequest::FsTransaction(request) => self.fs_transaction(request),
            AutonomousToolRequest::JsonEdit(request) => self.structured_edit(
                request,
                AutonomousStructuredEditFormat::Json,
                AUTONOMOUS_TOOL_JSON_EDIT,
            ),
            AutonomousToolRequest::TomlEdit(request) => self.structured_edit(
                request,
                AutonomousStructuredEditFormat::Toml,
                AUTONOMOUS_TOOL_TOML_EDIT,
            ),
            AutonomousToolRequest::YamlEdit(request) => self.structured_edit(
                request,
                AutonomousStructuredEditFormat::Yaml,
                AUTONOMOUS_TOOL_YAML_EDIT,
            ),
            AutonomousToolRequest::Delete(request) => self.delete(request),
            AutonomousToolRequest::Rename(request) => self.rename(request),
            AutonomousToolRequest::Mkdir(request) => self.mkdir(request),
            AutonomousToolRequest::List(request) => self.list(request),
            AutonomousToolRequest::ListTree(request) => self.list_tree(request),
            AutonomousToolRequest::DirectoryDigest(request) => self.directory_digest(request),
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
            AutonomousToolRequest::HostCommand(request) => self.host_command(request),
            AutonomousToolRequest::ProcessManager(request) => self.process_manager(request),
            AutonomousToolRequest::RuntimeWait(request) => self.runtime_wait(request),
            AutonomousToolRequest::ActionRequired(request) => self.action_required(request),
            AutonomousToolRequest::SuggestRouting(request) => self.suggest_routing(request),
            AutonomousToolRequest::SystemDiagnostics(request) => self.system_diagnostics(request),
            AutonomousToolRequest::MacosAutomation(request) => self.macos_automation(request),
            AutonomousToolRequest::DesktopObserve(request) => self.desktop_observe(request),
            AutonomousToolRequest::DesktopControl(request) => self.desktop_control(request),
            AutonomousToolRequest::DesktopStream(request) => self.desktop_stream(request),
            AutonomousToolRequest::Mcp(request) => self.mcp(request),
            AutonomousToolRequest::Subagent(request) => self.subagent(request),
            AutonomousToolRequest::RequestSensitiveInput(request) => {
                self.request_sensitive_input(request)
            }
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
            AutonomousToolRequest::WorkflowDefinition(request) => self.workflow_definition(request),
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

    fn request_sensitive_input(
        &self,
        request: AutonomousSensitiveInputRequest,
    ) -> CommandResult<AutonomousToolResult> {
        validate_sensitive_input_request(&request)?;
        let action_id = sensitive_input_action_id(&request)?;
        let required_count = request.fields.iter().filter(|field| field.required).count();
        let optional_count = request.fields.len().saturating_sub(required_count);
        let approved_values = take_sensitive_input_approval(&action_id);
        let approved = approved_values.is_some();
        let summary = format!(
            "{} {} sensitive field(s): {required_count} required, {optional_count} optional.",
            if approved { "Received" } else { "Requested" },
            request.fields.len()
        );

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_REQUEST_SENSITIVE_INPUT.into(),
            summary: summary.clone(),
            command_result: None,
            output: AutonomousToolOutput::SensitiveInput(AutonomousSensitiveInputOutput {
                action_id,
                status: if approved {
                    "approved".into()
                } else {
                    "pending_user_review".into()
                },
                purpose: request.purpose,
                intended_use: request.intended_use,
                allow_partial: request.allow_partial,
                fields: request
                    .fields
                    .into_iter()
                    .map(|field| AutonomousSensitiveInputFieldOutput {
                        value: approved_values
                            .as_ref()
                            .and_then(|values| values.get(&field.key))
                            .map(sensitive_input_value_to_string)
                            .unwrap_or_else(|| "[redacted]".into()),
                        key: field.key,
                        label: field.label,
                        description: field.description,
                        required: field.required,
                        validation_hint: field.validation_hint,
                    })
                    .collect(),
                redacted: !approved,
                summary,
            }),
        })
    }

    fn action_required(
        &self,
        request: AutonomousActionRequiredRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.check_cancelled()?;
        validate_action_required_request(&request)?;
        let context = self.agent_run_context.as_ref().ok_or_else(|| {
            CommandError::system_fault(
                "action_required_missing_run_context",
                "Xero cannot ask for user input without an active owned-agent run context.",
            )
        })?;
        let action_id = action_required_action_id(context, &request)?;
        let allow_multiple =
            request.answer_shape == AutonomousActionRequiredAnswerShape::MultiChoice;
        let title = request.title.trim().to_string();
        let detail = request.detail.trim().to_string();
        let intended_use = request.intended_use.map(|value| value.trim().to_string());
        let prompt_kind = request.prompt_kind.map(|value| value.trim().to_string());
        let summary = format!("Requested user input: {title}");

        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_ACTION_REQUIRED.into(),
            summary: summary.clone(),
            command_result: None,
            output: AutonomousToolOutput::ActionRequired(AutonomousActionRequiredOutput {
                action_id,
                action_type: "user_input_required".into(),
                status: "pending_user_response".into(),
                title,
                detail,
                answer_shape: request.answer_shape,
                prompt_kind,
                options: request.options,
                allow_multiple,
                intended_use,
                summary,
            }),
        })
    }

    fn suggest_routing(
        &self,
        request: AutonomousRouteRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.check_cancelled()?;
        if self.subagent_child_identity.is_some() {
            return Err(CommandError::policy_denied(
                "Subagents cannot request a route change for the parent conversation.",
            ));
        }
        validate_route_request(&request)?;
        let context = self.agent_run_context.as_ref().ok_or_else(|| {
            CommandError::system_fault(
                "route_request_missing_run_context",
                "Xero cannot validate a routing request without an active owned-agent run context.",
            )
        })?;
        let fallback_runtime_agent_id = request
            .target_agent_id
            .unwrap_or(RuntimeAgentIdDto::Generalist);
        let target = crate::runtime::agent_core::resolve_agent_route_target(
            &self.repo_root,
            &context.project_id,
            &context.run_id,
            fallback_runtime_agent_id,
            request.target_agent_definition_id.as_deref(),
            request.target_agent_definition_version,
        )?;

        if target.target_kind != request.target_kind.as_str() {
            return Err(CommandError::user_fixable(
                "agent_route_target_kind_mismatch",
                format!(
                    "Routing target `{}` resolves as `{}`, not `{}`.",
                    target.display_name,
                    target.target_kind,
                    request.target_kind.as_str(),
                ),
            ));
        }
        if request
            .target_agent_id
            .is_some_and(|agent_id| agent_id != target.runtime_agent_id)
        {
            return Err(CommandError::user_fixable(
                "agent_route_target_identity_mismatch",
                format!(
                    "Routing target `{}` resolves to runtime agent `{}`, not `{}`.",
                    target.display_name,
                    target.runtime_agent_id.as_str(),
                    fallback_runtime_agent_id.as_str(),
                ),
            ));
        }

        let reason = request.reason.trim().to_owned();
        let summary = request.summary.trim().to_owned();
        let request_id = route_request_id(context, &target, &reason, &summary);
        let message = format!("Requested routing to {}.", target.display_name);
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_SUGGEST_ROUTING.into(),
            summary: message.clone(),
            command_result: None,
            output: AutonomousToolOutput::RouteRequest(AutonomousRouteRequestOutput {
                schema: "xero.route_request.v1".into(),
                request_id,
                target_kind: request.target_kind,
                target_agent_id: target.runtime_agent_id,
                target_agent_definition_id: (target.target_kind == "custom")
                    .then_some(target.agent_definition_id),
                target_agent_definition_version: (target.target_kind == "custom")
                    .then_some(target.agent_definition_version),
                target_label: target.display_name,
                reason,
                summary,
                policy_decision: "approved".into(),
                auto_routable: target.auto_routable,
                message,
            }),
        })
    }

    fn runtime_wait(
        &self,
        request: AutonomousRuntimeWaitRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.check_cancelled()?;
        validate_runtime_wait_request(&request)?;
        let context = self.agent_run_context.as_ref().ok_or_else(|| {
            CommandError::system_fault(
                "runtime_wait_missing_run_context",
                "Xero cannot schedule a wait without an active owned-agent run context.",
            )
        })?;
        let now = OffsetDateTime::now_utc();
        let created_at = format_runtime_wait_timestamp(now)?;
        let delay_ms = runtime_wait_initial_delay_ms(&request);
        let due_at = format_runtime_wait_timestamp(add_runtime_wait_ms(now, delay_ms)?)?;
        let deadline_at = request
            .deadline_ms
            .map(|deadline_ms| add_runtime_wait_ms(now, deadline_ms))
            .transpose()?
            .map(format_runtime_wait_timestamp)
            .transpose()?;
        let reason = request.reason.trim().to_string();
        let process_id = request
            .process_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let output_pattern = request
            .output_pattern
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let payload = json!({
            "schema": "xero.agent_run_wakeup.payload.v1",
            "kind": request.kind,
            "reason": reason,
            "resumeContext": request.resume_context,
            "processId": process_id,
            "outputPattern": output_pattern,
            "pollIntervalMs": request.poll_interval_ms,
            "delayMs": request.delay_ms,
            "deadlineMs": request.deadline_ms,
            "scheduledAt": created_at,
        });
        let payload_json = serde_json::to_string(&payload).map_err(|error| {
            CommandError::system_fault(
                "runtime_wait_payload_serialize_failed",
                format!("Xero could not serialize scheduled wakeup payload: {error}"),
            )
        })?;
        let wake_id = runtime_wait_wake_id(context, &payload_json, &due_at, &created_at)?;
        let record = project_store::insert_agent_run_wakeup(
            &self.repo_root,
            &project_store::NewAgentRunWakeupRecord {
                project_id: context.project_id.clone(),
                agent_session_id: context.agent_session_id.clone(),
                run_id: context.run_id.clone(),
                wake_id: wake_id.clone(),
                kind: request.kind.as_wakeup_kind(),
                due_at: due_at.clone(),
                deadline_at: deadline_at.clone(),
                poll_interval_ms: request.poll_interval_ms,
                payload_json,
                created_at: created_at.clone(),
            },
        )?;
        crate::runtime::notify_agent_run_wakeup_inserted(&self.repo_root, &record, self.clone());

        let message = format!("Scheduled owned-agent wakeup `{wake_id}` for {due_at}: {reason}");
        Ok(AutonomousToolResult {
            tool_name: AUTONOMOUS_TOOL_RUNTIME_WAIT.into(),
            summary: message.clone(),
            command_result: None,
            output: AutonomousToolOutput::RuntimeWait(AutonomousRuntimeWaitOutput {
                wake_id,
                kind: request.kind,
                status: "scheduled".into(),
                due_at,
                deadline_at,
                poll_interval_ms: request.poll_interval_ms,
                process_id,
                reason,
                resume_context: request.resume_context,
                message,
            }),
        })
    }

    pub fn execute_approved(
        &self,
        request: AutonomousToolRequest,
    ) -> CommandResult<AutonomousToolResult> {
        self.check_cancelled()?;
        let tool_name = request.tool_name();
        self.enforce_agent_workflow_before_tool(tool_name)?;
        self.enforce_mailbox_check_before_mutation(tool_name, &request)?;
        let result = match request {
            AutonomousToolRequest::Read(request) => self.read_with_operator_approval(request),
            AutonomousToolRequest::Command(request) => self.command_with_operator_approval(request),
            AutonomousToolRequest::CommandSessionStart(request) => {
                self.command_session_start_with_operator_approval(request)
            }
            AutonomousToolRequest::HostCommand(request) => {
                self.host_command_with_operator_approval(request)
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
            AutonomousToolRequest::DesktopObserve(request) => {
                self.desktop_observe_with_operator_approval(request)
            }
            AutonomousToolRequest::DesktopControl(request) => {
                self.desktop_control_with_operator_approval(request)
            }
            AutonomousToolRequest::DesktopStream(request) => {
                self.desktop_stream_with_operator_approval(request)
            }
            AutonomousToolRequest::AgentDefinition(request) => {
                self.agent_definition_with_operator_approval(request)
            }
            AutonomousToolRequest::WorkflowDefinition(request) => {
                self.workflow_definition_with_operator_approval(request)
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
        let output = Self::redact_solana_output_for_agent(run(executor.as_ref())?);
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

    fn redact_solana_output_for_agent(
        mut output: AutonomousSolanaOutput,
    ) -> AutonomousSolanaOutput {
        match serde_json::from_str::<JsonValue>(&output.value_json) {
            Ok(value) => {
                let redacted = Self::redact_solana_json_for_agent(value, None);
                output.value_json =
                    serde_json::to_string(&redacted).unwrap_or_else(|_| "null".into());
            }
            Err(_) if find_prohibited_persistence_content(&output.value_json).is_some() => {
                output.value_json = "\"[REDACTED]\"".into();
            }
            Err(_) => {}
        }
        output
    }

    fn redact_solana_json_for_agent(value: JsonValue, key: Option<&str>) -> JsonValue {
        match value {
            JsonValue::String(text) => {
                JsonValue::String(Self::redact_solana_text_for_agent(key, text))
            }
            JsonValue::Array(items) => JsonValue::Array(
                items
                    .into_iter()
                    .map(|item| Self::redact_solana_json_for_agent(item, key))
                    .collect(),
            ),
            JsonValue::Object(fields) => JsonValue::Object(
                fields
                    .into_iter()
                    .map(|(field_key, field_value)| {
                        let value = if Self::is_sensitive_solana_result_key(&field_key) {
                            JsonValue::String("[REDACTED]".into())
                        } else {
                            Self::redact_solana_json_for_agent(field_value, Some(&field_key))
                        };
                        (field_key, value)
                    })
                    .collect(),
            ),
            other => other,
        }
    }

    fn redact_solana_text_for_agent(key: Option<&str>, text: String) -> String {
        if key.is_some_and(Self::is_solana_url_result_key) {
            return crate::commands::solana::provider_profiles::redact_url(&text);
        }

        if key.is_some_and(Self::is_solana_free_text_result_key)
            && find_prohibited_persistence_content(&text).is_some()
        {
            "[REDACTED]".into()
        } else {
            text
        }
    }

    fn is_sensitive_solana_result_key(key: &str) -> bool {
        let normalized = Self::normalize_solana_result_key(key);
        normalized.contains("keypair")
            || normalized.contains("privatekey")
            || normalized.contains("seedphrase")
            || normalized.contains("walletmaterial")
            || normalized.contains("authorization")
            || normalized.contains("credential")
            || normalized.contains("apikey")
            || normalized.contains("secret")
            || normalized.contains("token")
            || normalized.contains("screenshot")
            || normalized.contains("imagebase64")
            || normalized.contains("pngbase64")
            || normalized.contains("rawpayload")
            || normalized.contains("toolpayload")
            || normalized.contains("telemetrypayload")
            || normalized.contains("diagnosticbundle")
    }

    fn is_solana_url_result_key(key: &str) -> bool {
        matches!(
            Self::normalize_solana_result_key(key).as_str(),
            "rpcurl" | "websocketurl" | "endpointurl" | "providerurl" | "url"
        )
    }

    fn is_solana_free_text_result_key(key: &str) -> bool {
        matches!(
            Self::normalize_solana_result_key(key).as_str(),
            "message"
                | "messages"
                | "error"
                | "errors"
                | "diagnostic"
                | "diagnostics"
                | "exporteddiagnostics"
                | "evidence"
                | "summary"
                | "details"
                | "stdout"
                | "stderr"
                | "log"
                | "logs"
        )
    }

    fn normalize_solana_result_key(key: &str) -> String {
        key.chars()
            .filter(|ch| ch.is_ascii_alphanumeric())
            .flat_map(|ch| ch.to_lowercase())
            .collect()
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
        let output = executor.execute(
            request.action,
            BrowserExecutionContext {
                preference: self.browser_control_preference,
                project_id: self
                    .agent_run_context
                    .as_ref()
                    .map(|context| context.project_id.clone()),
                repo_root: self.repo_root.clone(),
            },
        )?;
        let output_envelope = serde_json::from_str::<JsonValue>(&output.value_json).ok();
        let summary = output_envelope
            .as_ref()
            .and_then(|value| value.get("summary"))
            .and_then(JsonValue::as_str)
            .map(str::to_owned)
            .or_else(|| {
                output
                    .url
                    .as_ref()
                    .map(|url| format!("Executed browser action `{}` on `{}`.", output.action, url))
            })
            .unwrap_or_else(|| {
                format!(
                    "Executed browser action `{}` ({action_summary}).",
                    output.action
                )
            });
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
                granted_tool_details: Vec::new(),
                denied_tools: Vec::new(),
                available_groups: self.available_tool_access_groups(),
                available_tool_packs: self.available_tool_pack_manifests(),
                tool_pack_health: self.tool_pack_health_reports(),
                capability_manifest: Some(self.computer_use_capability_manifest()),
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
                                    && self.tool_allowed_by_active_agent_and_stage(tool)
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
                        && self.tool_allowed_by_active_agent_and_stage(tool.as_str());
                    let dynamic_tool_available =
                        self.active_runtime_agent_id().allows_engineering_tools()
                            && self
                                .agent_tool_policy
                                .as_ref()
                                .map(|policy| policy.allows_tool(&tool))
                                .unwrap_or(true)
                            && self.tool_allowed_by_current_workflow_phase(&tool)
                            && self.dynamic_tool_descriptor(&tool)?.is_some();
                    if runtime_tool_available || dynamic_tool_available {
                        requested.insert(tool);
                    } else {
                        denied.insert(tool);
                    }
                }

                let granted_tools = requested.into_iter().collect::<Vec<_>>();
                let granted_tool_details = granted_tools
                    .iter()
                    .map(|tool| self.tool_access_tool_summary(tool))
                    .collect::<Vec<_>>();

                AutonomousToolAccessOutput {
                    action: "request".into(),
                    granted_tools,
                    granted_tool_details,
                    denied_tools: denied.into_iter().collect(),
                    available_groups: Vec::new(),
                    available_tool_packs: Vec::new(),
                    tool_pack_health: Vec::new(),
                    capability_manifest: None,
                    exposure_diagnostics: Some(Self::tool_access_exposure_diagnostics(
                        request.reason.as_deref(),
                    )),
                    message: "Requested tools will be exposed on the next provider turn. Use action=list only when you need the full available tool catalog.".into(),
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
                    self.tool_available_by_runtime(tool)
                        && self.tool_allowed_by_active_agent_and_stage(tool)
                });
                group.tool_summaries = group
                    .tools
                    .iter()
                    .map(|tool| self.tool_access_tool_summary(tool))
                    .collect();
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

    fn tool_access_tool_summary(&self, tool: &str) -> AutonomousToolAccessToolSummary {
        let risk_class = tool_catalog_metadata_for_tool(tool, self.skill_tool_enabled())
            .and_then(|metadata| {
                metadata
                    .get("riskClass")
                    .and_then(JsonValue::as_str)
                    .map(ToOwned::to_owned)
            })
            .or_else(|| {
                tool_catalog_activation_groups(tool)
                    .first()
                    .and_then(|group| tool_access_group_definition(group))
                    .map(|definition| definition.risk_class.to_owned())
            })
            .unwrap_or_else(|| "unknown".into());
        AutonomousToolAccessToolSummary {
            tool_name: tool.to_owned(),
            effect_class: tool_effect_class(tool).as_str().into(),
            risk_class,
            runtime_available: self.tool_available_by_runtime(tool),
            allowed_for_agent: self.tool_allowed_by_active_agent_and_stage(tool),
            activation_groups: tool_catalog_activation_groups(tool),
        }
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
            self.tool_available_by_runtime(tool)
                && self.tool_allowed_by_active_agent_and_stage(tool)
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

    fn computer_use_capability_manifest(&self) -> JsonValue {
        let computer_use_policy = AutonomousAgentToolPolicy::from_policy_label("computer_use");
        let default_tools = computer_use_default_tool_names();
        let tools = deferred_tool_catalog(self.skill_tool_enabled())
            .into_iter()
            .map(|entry| {
                let tool_name = entry.tool_name;
                let allowed_by_policy = tool_allowed_for_runtime_agent_with_policy(
                    RuntimeAgentIdDto::ComputerUse,
                    tool_name,
                    Some(&computer_use_policy),
                );
                let host_available = tool_available_on_current_host(tool_name);
                let runtime_available = self.tool_available_by_runtime(tool_name);
                let active_by_default =
                    allowed_by_policy && runtime_available && default_tools.contains(&tool_name);
                let activation_groups = tool_catalog_activation_groups(tool_name);
                let available_through_tool_access = allowed_by_policy
                    && runtime_available
                    && !active_by_default
                    && !activation_groups.is_empty();
                let availability_context = ComputerUseManifestAvailabilityContext {
                    tool_name,
                    allowed_by_policy,
                    host_available,
                    runtime_available,
                    active_by_default,
                    available_through_tool_access,
                    activation_groups: &activation_groups,
                    skill_tool_enabled: self.skill_tool_enabled(),
                };
                let availability_diagnostics =
                    computer_use_manifest_availability_diagnostics(availability_context);
                json!({
                    "toolName": tool_name,
                    "group": entry.group,
                    "description": entry.description,
                    "riskClass": entry.risk_class,
                    "effectClass": tool_effect_class(tool_name).as_str(),
                    "schemaFields": entry.schema_fields,
                    "actions": computer_use_manifest_actions_for_tool(tool_name),
                    "toolPackIds": domain_tool_pack_ids_for_tool(tool_name),
                    "allowedByComputerUsePolicy": allowed_by_policy,
                    "runtimeAvailable": runtime_available,
                    "defaultAvailability": if active_by_default { "active_by_default" } else if available_through_tool_access { "available_through_tool_access" } else { "not_available" },
                    "toolAccessAvailability": if available_through_tool_access { "requestable" } else if active_by_default { "already_default" } else { "unavailable" },
                    "activationGroups": activation_groups,
                    "rolloutGate": computer_use_manifest_rollout_gate(tool_name),
                    "platformGate": computer_use_manifest_platform_gate(tool_name),
                    "approvalRequirement": computer_use_manifest_approval_requirement(tool_name),
                    "availabilityReasons": computer_use_manifest_availability_reasons(
                        allowed_by_policy,
                        host_available,
                        runtime_available,
                        active_by_default,
                        available_through_tool_access,
                    ),
                    "availabilityDiagnostics": availability_diagnostics,
                })
            })
            .collect::<Vec<_>>();

        json!({
            "schema": "xero.computer_use_capability_manifest.v1",
            "generatedAt": crate::auth::now_timestamp(),
            "host": runtime_host_metadata(),
            "runtimeAgentId": RuntimeAgentIdDto::ComputerUse.as_str(),
            "policyProfile": "computer_use",
            "surfaces": {
                "manualWebRtcControl": "Manual cloud streaming and pointer forwarding use the desktop broker but are separate from LLM-chosen Computer Use tools.",
                "llmDrivenTools": "Computer Use chooses among repository, command, browser, desktop, diagnostics, MCP, skill, subagent, context, and domain tools exposed for the current turn.",
            },
            "workstationControlPack": {
                "requiredFamilies": [
                    "desktop_sidecar",
                    "browser_control",
                    "host_command_admin",
                    "clipboard_file_drop",
                    "ocr_ui_tree",
                    "diagnostics"
                ],
                "status": workstation_control_pack_status(self),
                "capabilityReportFixtures": workstation_capability_report_fixtures(),
            },
            "tools": tools,
        })
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

    fn tool_allowed_by_active_agent_and_stage(&self, tool: &str) -> bool {
        self.tool_allowed_by_active_agent(tool) && self.tool_allowed_by_current_workflow_phase(tool)
    }

    fn tool_allowed_by_current_workflow_phase(&self, tool: &str) -> bool {
        let Some(policy) = self.agent_workflow_policy.as_ref() else {
            return true;
        };
        let Ok(todos) = self.todo_items.lock() else {
            return false;
        };
        let Ok(mut state) = self.agent_workflow_state.lock() else {
            return false;
        };
        policy.advance_state(&mut state, &todos);
        policy
            .phase(&state.current_phase_id)
            .is_some_and(|phase| phase.allows_tool(tool))
    }

    pub(crate) fn current_workflow_allowed_tools(&self) -> CommandResult<Option<BTreeSet<String>>> {
        let Some(policy) = self.agent_workflow_policy.as_ref() else {
            return Ok(None);
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
        Ok(policy
            .phase(&state.current_phase_id)
            .and_then(AutonomousAgentWorkflowPhase::registry_allowed_tools))
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case", tag = "tool", content = "input")]
pub enum AutonomousToolRequest {
    Read(AutonomousReadRequest),
    ReadMany(AutonomousReadManyRequest),
    ResultPage(AutonomousResultPageRequest),
    Stat(AutonomousStatRequest),
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
    Copy(AutonomousCopyRequest),
    FsTransaction(AutonomousFsTransactionRequest),
    JsonEdit(AutonomousStructuredEditRequest),
    TomlEdit(AutonomousStructuredEditRequest),
    YamlEdit(AutonomousStructuredEditRequest),
    Delete(AutonomousDeleteRequest),
    Rename(AutonomousRenameRequest),
    Mkdir(AutonomousMkdirRequest),
    List(AutonomousListRequest),
    ListTree(AutonomousListTreeRequest),
    DirectoryDigest(AutonomousDirectoryDigestRequest),
    #[serde(rename = "file_hash")]
    Hash(AutonomousHashRequest),
    Command(AutonomousCommandRequest),
    CommandSessionStart(AutonomousCommandSessionStartRequest),
    CommandSessionRead(AutonomousCommandSessionReadRequest),
    CommandSessionStop(AutonomousCommandSessionStopRequest),
    HostCommand(AutonomousHostCommandRequest),
    ProcessManager(AutonomousProcessManagerRequest),
    RuntimeWait(AutonomousRuntimeWaitRequest),
    ActionRequired(AutonomousActionRequiredRequest),
    SuggestRouting(AutonomousRouteRequest),
    SystemDiagnostics(AutonomousSystemDiagnosticsRequest),
    MacosAutomation(AutonomousMacosAutomationRequest),
    DesktopObserve(AutonomousDesktopObserveRequest),
    DesktopControl(AutonomousDesktopControlRequest),
    DesktopStream(AutonomousDesktopStreamRequest),
    Mcp(AutonomousMcpRequest),
    Subagent(AutonomousSubagentRequest),
    RequestSensitiveInput(AutonomousSensitiveInputRequest),
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
    WorkflowDefinition(AutonomousWorkflowDefinitionRequest),
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
pub struct AutonomousSensitiveInputFieldRequest {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_sensitive_input_field_required")]
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSensitiveInputRequest {
    pub purpose: String,
    pub intended_use: String,
    pub fields: Vec<AutonomousSensitiveInputFieldRequest>,
    #[serde(default)]
    pub allow_partial: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSensitiveInputFieldOutput {
    pub key: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_hint: Option<String>,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSensitiveInputOutput {
    pub action_id: String,
    pub status: String,
    pub purpose: String,
    pub intended_use: String,
    pub allow_partial: bool,
    pub fields: Vec<AutonomousSensitiveInputFieldOutput>,
    pub redacted: bool,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousActionRequiredAnswerShape {
    PlainText,
    TerminalInput,
    SingleChoice,
    MultiChoice,
    ShortText,
    LongText,
    Number,
    Date,
}

impl AutonomousActionRequiredAnswerShape {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PlainText => "plain_text",
            Self::TerminalInput => "terminal_input",
            Self::SingleChoice => "single_choice",
            Self::MultiChoice => "multi_choice",
            Self::ShortText => "short_text",
            Self::LongText => "long_text",
            Self::Number => "number",
            Self::Date => "date",
        }
    }

    const fn requires_options(self) -> bool {
        matches!(self, Self::SingleChoice | Self::MultiChoice)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousActionRequiredOption {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousActionRequiredRequest {
    pub title: String,
    pub detail: String,
    pub answer_shape: AutonomousActionRequiredAnswerShape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<AutonomousActionRequiredOption>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intended_use: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousActionRequiredOutput {
    pub action_id: String,
    pub action_type: String,
    pub status: String,
    pub title: String,
    pub detail: String,
    pub answer_shape: AutonomousActionRequiredAnswerShape,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<AutonomousActionRequiredOption>,
    pub allow_multiple: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intended_use: Option<String>,
    pub summary: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousRouteTargetKind {
    BuiltIn,
    Custom,
}

impl AutonomousRouteTargetKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built_in",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRouteRequest {
    pub target_kind: AutonomousRouteTargetKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_id: Option<RuntimeAgentIdDto>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_definition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_definition_version: Option<u32>,
    pub reason: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRouteRequestOutput {
    pub schema: String,
    pub request_id: String,
    pub target_kind: AutonomousRouteTargetKind,
    pub target_agent_id: RuntimeAgentIdDto,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_definition_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_agent_definition_version: Option<u32>,
    pub target_label: String,
    pub reason: String,
    pub summary: String,
    pub policy_decision: String,
    pub auto_routable: bool,
    pub message: String,
}

fn validate_route_request(request: &AutonomousRouteRequest) -> CommandResult<()> {
    let reason = request.reason.trim();
    if reason.len() < 4 || reason.len() > MAX_ROUTE_REQUEST_REASON_BYTES {
        return Err(CommandError::user_fixable(
            "agent_route_reason_invalid",
            format!(
                "`reason` must be between 4 and {MAX_ROUTE_REQUEST_REASON_BYTES} UTF-8 bytes after trimming."
            ),
        ));
    }
    let summary = request.summary.trim();
    if summary.len() < 4 || summary.len() > MAX_ROUTE_REQUEST_SUMMARY_BYTES {
        return Err(CommandError::user_fixable(
            "agent_route_summary_invalid",
            format!(
                "`summary` must be between 4 and {MAX_ROUTE_REQUEST_SUMMARY_BYTES} UTF-8 bytes after trimming."
            ),
        ));
    }
    if find_prohibited_persistence_content(reason).is_some()
        || find_prohibited_persistence_content(summary).is_some()
    {
        return Err(CommandError::user_fixable(
            "agent_route_content_secret_like",
            "Routing reasons and summaries must not contain secret-like content because route events are durably persisted.",
        ));
    }

    match request.target_kind {
        AutonomousRouteTargetKind::BuiltIn => {
            if request.target_agent_id.is_none() {
                return Err(CommandError::user_fixable(
                    "agent_route_target_id_required",
                    "Built-in routing requests require `targetAgentId`.",
                ));
            }
            if request.target_agent_definition_id.is_some()
                || request.target_agent_definition_version.is_some()
            {
                return Err(CommandError::user_fixable(
                    "agent_route_builtin_definition_forbidden",
                    "Built-in routing requests must not include custom definition identity fields.",
                ));
            }
        }
        AutonomousRouteTargetKind::Custom => {
            let definition_id = request
                .target_agent_definition_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default();
            if definition_id.is_empty() {
                return Err(CommandError::user_fixable(
                    "agent_route_definition_id_required",
                    "Custom routing requests require `targetAgentDefinitionId`.",
                ));
            }
        }
    }
    Ok(())
}

fn route_request_id(
    context: &AutonomousAgentRunContext,
    target: &crate::runtime::agent_core::ResolvedAgentRouteTarget,
    reason: &str,
    summary: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(context.project_id.as_bytes());
    hasher.update(context.agent_session_id.as_bytes());
    hasher.update(context.run_id.as_bytes());
    hasher.update(target.agent_definition_id.as_bytes());
    hasher.update(target.agent_definition_version.to_le_bytes());
    hasher.update(reason.as_bytes());
    hasher.update(summary.as_bytes());
    hasher.update(
        OffsetDateTime::now_utc()
            .unix_timestamp_nanos()
            .to_le_bytes(),
    );
    let digest = format!("{:x}", hasher.finalize());
    format!("route-{}", &digest[..20])
}

const fn default_sensitive_input_field_required() -> bool {
    true
}

fn validate_sensitive_input_request(
    request: &AutonomousSensitiveInputRequest,
) -> CommandResult<()> {
    validate_sensitive_input_text(&request.purpose, "purpose", 20, 500)?;
    validate_sensitive_input_text(&request.intended_use, "intendedUse", 10, 500)?;
    if request.fields.is_empty() || request.fields.len() > 12 {
        return Err(CommandError::user_fixable(
            "sensitive_input_fields_invalid",
            "Sensitive input requests must include between 1 and 12 fields.",
        ));
    }

    let mut keys = BTreeSet::new();
    for field in &request.fields {
        validate_sensitive_input_key(&field.key)?;
        validate_sensitive_input_text(&field.label, "field.label", 1, 120)?;
        if let Some(description) = field.description.as_deref() {
            validate_sensitive_input_text(description, "field.description", 1, 300)?;
        }
        if let Some(validation_hint) = field.validation_hint.as_deref() {
            validate_sensitive_input_text(validation_hint, "field.validationHint", 1, 300)?;
        }
        if !keys.insert(field.key.clone()) {
            return Err(CommandError::user_fixable(
                "sensitive_input_field_duplicate",
                format!(
                    "Sensitive input request contains duplicate field key `{}`.",
                    field.key
                ),
            ));
        }
    }

    if !request.allow_partial && request.fields.iter().any(|field| !field.required) {
        return Err(CommandError::user_fixable(
            "sensitive_input_partial_policy_invalid",
            "Sensitive input requests with optional fields must set allowPartial=true.",
        ));
    }

    Ok(())
}

fn validate_sensitive_input_text(
    value: &str,
    field: &'static str,
    min_chars: usize,
    max_chars: usize,
) -> CommandResult<()> {
    let trimmed = value.trim();
    if trimmed.len() < min_chars || trimmed.len() > max_chars {
        return Err(CommandError::user_fixable(
            "sensitive_input_request_invalid",
            format!(
                "`{field}` must be between {min_chars} and {max_chars} UTF-8 bytes after trimming."
            ),
        ));
    }
    if find_prohibited_persistence_content(trimmed).is_some() {
        return Err(CommandError::user_fixable(
            "sensitive_input_metadata_secret_like",
            format!("`{field}` must describe the request without embedding secret values."),
        ));
    }
    Ok(())
}

fn validate_sensitive_input_key(key: &str) -> CommandResult<()> {
    let valid = !key.is_empty()
        && key.len() <= 80
        && key
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(CommandError::user_fixable(
            "sensitive_input_field_key_invalid",
            "Sensitive input field keys must be lowercase snake_case identifiers up to 80 bytes.",
        ))
    }
}

fn sensitive_input_action_id(request: &AutonomousSensitiveInputRequest) -> CommandResult<String> {
    let bytes = serde_json::to_vec(request).map_err(|error| {
        CommandError::system_fault(
            "sensitive_input_hash_failed",
            format!("Xero could not hash sensitive input request metadata: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let digest = format!("{:x}", hasher.finalize());
    Ok(format!("sensitive-input-{}", &digest[..16]))
}

fn sensitive_input_value_to_string(value: &JsonValue) -> String {
    value
        .as_str()
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn validate_action_required_request(
    request: &AutonomousActionRequiredRequest,
) -> CommandResult<()> {
    validate_action_required_text(&request.title, "title", 3, 120)?;
    validate_action_required_text(
        &request.detail,
        "detail",
        8,
        MAX_ACTION_REQUIRED_DETAIL_BYTES,
    )?;
    if let Some(prompt_kind) = request.prompt_kind.as_deref() {
        validate_action_required_key(prompt_kind, "promptKind")?;
    }
    if let Some(intended_use) = request.intended_use.as_deref() {
        validate_action_required_text(intended_use, "intendedUse", 8, 500)?;
    }

    if request.answer_shape.requires_options() {
        if request.options.len() < 2 || request.options.len() > MAX_ACTION_REQUIRED_OPTIONS {
            return Err(CommandError::user_fixable(
                "action_required_options_invalid",
                format!(
                    "Choice prompts must include between 2 and {MAX_ACTION_REQUIRED_OPTIONS} options."
                ),
            ));
        }
    } else if !request.options.is_empty() {
        return Err(CommandError::user_fixable(
            "action_required_options_unexpected",
            "`options` can be provided only for single_choice and multi_choice prompts.",
        ));
    }

    let mut ids = BTreeSet::new();
    for option in &request.options {
        validate_action_required_option_id(&option.id)?;
        validate_action_required_text(&option.label, "option.label", 1, 120)?;
        if let Some(description) = option.description.as_deref() {
            validate_action_required_text(description, "option.description", 1, 300)?;
        }
        if !ids.insert(option.id.trim().to_string()) {
            return Err(CommandError::user_fixable(
                "action_required_option_duplicate",
                format!(
                    "Choice prompt contains duplicate option id `{}`.",
                    option.id
                ),
            ));
        }
    }

    Ok(())
}

fn validate_action_required_text(
    value: &str,
    field: &'static str,
    min_chars: usize,
    max_chars: usize,
) -> CommandResult<()> {
    let trimmed = value.trim();
    if trimmed.len() < min_chars || trimmed.len() > max_chars {
        return Err(CommandError::user_fixable(
            "action_required_request_invalid",
            format!(
                "`{field}` must be between {min_chars} and {max_chars} UTF-8 bytes after trimming."
            ),
        ));
    }
    if find_prohibited_persistence_content(trimmed).is_some() {
        return Err(CommandError::user_fixable(
            "action_required_metadata_secret_like",
            format!("`{field}` must describe the prompt without embedding secret values."),
        ));
    }
    Ok(())
}

fn validate_action_required_key(value: &str, field: &'static str) -> CommandResult<()> {
    let trimmed = value.trim();
    let valid = !trimmed.is_empty()
        && trimmed.len() <= 80
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_');
    if valid {
        Ok(())
    } else {
        Err(CommandError::user_fixable(
            "action_required_key_invalid",
            format!("`{field}` must be a lowercase snake_case identifier up to 80 bytes."),
        ))
    }
}

fn validate_action_required_option_id(id: &str) -> CommandResult<()> {
    let trimmed = id.trim();
    let valid = !trimmed.is_empty()
        && trimmed.len() <= 80
        && trimmed
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(CommandError::user_fixable(
            "action_required_option_id_invalid",
            "Choice option ids must contain only ASCII letters, numbers, hyphen, underscore, or dot, up to 80 bytes.",
        ))
    }
}

fn action_required_action_id(
    context: &AutonomousAgentRunContext,
    request: &AutonomousActionRequiredRequest,
) -> CommandResult<String> {
    let bytes = serde_json::to_vec(request).map_err(|error| {
        CommandError::system_fault(
            "action_required_hash_failed",
            format!("Xero could not hash action-required request metadata: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(context.project_id.as_bytes());
    hasher.update(context.agent_session_id.as_bytes());
    hasher.update(context.run_id.as_bytes());
    hasher.update(bytes);
    let digest = format!("{:x}", hasher.finalize());
    Ok(format!("user-input-{}", &digest[..16]))
}

fn validate_runtime_wait_request(request: &AutonomousRuntimeWaitRequest) -> CommandResult<()> {
    validate_runtime_wait_duration(
        runtime_wait_initial_delay_ms(request),
        "delayMs",
        MAX_RUNTIME_WAIT_DELAY_MS,
    )?;
    let reason = request.reason.trim();
    if reason.len() < 8 || reason.len() > MAX_RUNTIME_WAIT_REASON_BYTES {
        return Err(CommandError::user_fixable(
            "runtime_wait_reason_invalid",
            format!(
                "`reason` must be between 8 and {MAX_RUNTIME_WAIT_REASON_BYTES} UTF-8 bytes after trimming."
            ),
        ));
    }
    if find_prohibited_persistence_content(reason).is_some() {
        return Err(CommandError::user_fixable(
            "runtime_wait_reason_secret_like",
            "`reason` must not contain secret-like content because scheduled waits are durably persisted.",
        ));
    }

    let resume_context_json = serde_json::to_string(&request.resume_context).map_err(|error| {
        CommandError::user_fixable(
            "runtime_wait_resume_context_invalid",
            format!("Xero could not serialize resumeContext: {error}"),
        )
    })?;
    if !request.resume_context.is_object() {
        return Err(CommandError::user_fixable(
            "runtime_wait_resume_context_invalid",
            "`resumeContext` must be a JSON object.",
        ));
    }
    if resume_context_json.len() > MAX_RUNTIME_WAIT_RESUME_CONTEXT_BYTES {
        return Err(CommandError::user_fixable(
            "runtime_wait_resume_context_too_large",
            format!(
                "`resumeContext` must be at most {MAX_RUNTIME_WAIT_RESUME_CONTEXT_BYTES} UTF-8 bytes."
            ),
        ));
    }
    if find_prohibited_persistence_content(&resume_context_json).is_some() {
        return Err(CommandError::user_fixable(
            "runtime_wait_resume_context_secret_like",
            "`resumeContext` must not contain secret-like content because scheduled waits are durably persisted.",
        ));
    }

    match request.kind {
        AutonomousRuntimeWaitKind::Sleep => {
            if request.delay_ms.is_none() {
                return Err(CommandError::user_fixable(
                    "runtime_wait_delay_required",
                    "`delayMs` is required for sleep wakeups.",
                ));
            }
        }
        AutonomousRuntimeWaitKind::ProcessExit
        | AutonomousRuntimeWaitKind::ProcessReady
        | AutonomousRuntimeWaitKind::ProcessOutput => {
            let process_id = request
                .process_id
                .as_deref()
                .map(str::trim)
                .unwrap_or_default();
            if process_id.is_empty() {
                return Err(CommandError::user_fixable(
                    "runtime_wait_process_id_required",
                    "`processId` is required for process wakeups.",
                ));
            }
            if request.deadline_ms.is_none() {
                return Err(CommandError::user_fixable(
                    "runtime_wait_deadline_required",
                    "`deadlineMs` is required for process wakeups so polling is bounded.",
                ));
            }
        }
    }

    if let Some(delay_ms) = request.delay_ms {
        validate_runtime_wait_duration(delay_ms, "delayMs", MAX_RUNTIME_WAIT_DELAY_MS)?;
    }
    if let Some(poll_interval_ms) = request.poll_interval_ms {
        validate_runtime_wait_duration(
            poll_interval_ms,
            "pollIntervalMs",
            MAX_RUNTIME_WAIT_DELAY_MS,
        )?;
    }
    if let Some(deadline_ms) = request.deadline_ms {
        validate_runtime_wait_duration(deadline_ms, "deadlineMs", MAX_RUNTIME_WAIT_DEADLINE_MS)?;
    }
    if request.kind == AutonomousRuntimeWaitKind::ProcessOutput {
        let pattern = request
            .output_pattern
            .as_deref()
            .map(str::trim)
            .unwrap_or_default();
        if pattern.is_empty() || pattern.len() > 500 {
            return Err(CommandError::user_fixable(
                "runtime_wait_output_pattern_invalid",
                "`outputPattern` must be 1 to 500 UTF-8 bytes for process_output wakeups.",
            ));
        }
        regex::Regex::new(pattern).map_err(|error| {
            CommandError::user_fixable(
                "runtime_wait_output_pattern_invalid",
                format!("`outputPattern` must be a valid regex: {error}"),
            )
        })?;
    }
    Ok(())
}

fn validate_runtime_wait_duration(
    value: u64,
    field: &'static str,
    max_value: u64,
) -> CommandResult<()> {
    if !(MIN_RUNTIME_WAIT_DELAY_MS..=max_value).contains(&value) {
        return Err(CommandError::user_fixable(
            "runtime_wait_duration_out_of_range",
            format!(
                "`{field}` must be between {MIN_RUNTIME_WAIT_DELAY_MS} and {max_value} milliseconds."
            ),
        ));
    }
    Ok(())
}

fn runtime_wait_initial_delay_ms(request: &AutonomousRuntimeWaitRequest) -> u64 {
    request
        .delay_ms
        .or(request.poll_interval_ms)
        .unwrap_or(DEFAULT_RUNTIME_WAIT_POLL_INTERVAL_MS)
}

fn add_runtime_wait_ms(
    timestamp: OffsetDateTime,
    duration_ms: u64,
) -> CommandResult<OffsetDateTime> {
    let duration_ms =
        i64::try_from(duration_ms).map_err(|_| CommandError::invalid_request("durationMs"))?;
    timestamp
        .checked_add(TimeDuration::milliseconds(duration_ms))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "runtime_wait_timestamp_out_of_range",
                "Scheduled wakeup timestamp is outside the supported range.",
            )
        })
}

fn format_runtime_wait_timestamp(timestamp: OffsetDateTime) -> CommandResult<String> {
    timestamp.format(&Rfc3339).map_err(|error| {
        CommandError::system_fault(
            "runtime_wait_timestamp_format_failed",
            format!("Xero could not format scheduled wakeup timestamp: {error}"),
        )
    })
}

fn runtime_wait_wake_id(
    context: &AutonomousAgentRunContext,
    payload_json: &str,
    due_at: &str,
    created_at: &str,
) -> CommandResult<String> {
    let mut hasher = Sha256::new();
    hasher.update(context.project_id.as_bytes());
    hasher.update(context.agent_session_id.as_bytes());
    hasher.update(context.run_id.as_bytes());
    hasher.update(payload_json.as_bytes());
    hasher.update(due_at.as_bytes());
    hasher.update(created_at.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    Ok(format!("wake-{}", &digest[..16]))
}

impl AutonomousToolRequest {
    pub fn tool_name(&self) -> &'static str {
        match self {
            Self::Read(_) => AUTONOMOUS_TOOL_READ,
            Self::ReadMany(_) => AUTONOMOUS_TOOL_READ_MANY,
            Self::ResultPage(_) => AUTONOMOUS_TOOL_RESULT_PAGE,
            Self::Stat(_) => AUTONOMOUS_TOOL_STAT,
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
            Self::Copy(_) => AUTONOMOUS_TOOL_COPY,
            Self::FsTransaction(_) => AUTONOMOUS_TOOL_FS_TRANSACTION,
            Self::JsonEdit(_) => AUTONOMOUS_TOOL_JSON_EDIT,
            Self::TomlEdit(_) => AUTONOMOUS_TOOL_TOML_EDIT,
            Self::YamlEdit(_) => AUTONOMOUS_TOOL_YAML_EDIT,
            Self::Delete(_) => AUTONOMOUS_TOOL_DELETE,
            Self::Rename(_) => AUTONOMOUS_TOOL_RENAME,
            Self::Mkdir(_) => AUTONOMOUS_TOOL_MKDIR,
            Self::List(_) => AUTONOMOUS_TOOL_LIST,
            Self::ListTree(_) => AUTONOMOUS_TOOL_LIST_TREE,
            Self::DirectoryDigest(_) => AUTONOMOUS_TOOL_DIRECTORY_DIGEST,
            Self::Hash(_) => AUTONOMOUS_TOOL_HASH,
            Self::Command(_) => AUTONOMOUS_TOOL_COMMAND,
            Self::CommandSessionStart(_) => AUTONOMOUS_TOOL_COMMAND_SESSION_START,
            Self::CommandSessionRead(_) => AUTONOMOUS_TOOL_COMMAND_SESSION_READ,
            Self::CommandSessionStop(_) => AUTONOMOUS_TOOL_COMMAND_SESSION_STOP,
            Self::HostCommand(_) => AUTONOMOUS_TOOL_HOST_COMMAND,
            Self::ProcessManager(_) => AUTONOMOUS_TOOL_PROCESS_MANAGER,
            Self::RuntimeWait(_) => AUTONOMOUS_TOOL_RUNTIME_WAIT,
            Self::ActionRequired(_) => AUTONOMOUS_TOOL_ACTION_REQUIRED,
            Self::SuggestRouting(_) => AUTONOMOUS_TOOL_SUGGEST_ROUTING,
            Self::SystemDiagnostics(_) => AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS,
            Self::MacosAutomation(_) => AUTONOMOUS_TOOL_MACOS_AUTOMATION,
            Self::DesktopObserve(_) => AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
            Self::DesktopControl(_) => AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            Self::DesktopStream(_) => AUTONOMOUS_TOOL_DESKTOP_STREAM,
            Self::Mcp(_) => AUTONOMOUS_TOOL_MCP,
            Self::Subagent(_) => AUTONOMOUS_TOOL_SUBAGENT,
            Self::RequestSensitiveInput(_) => AUTONOMOUS_TOOL_REQUEST_SENSITIVE_INPUT,
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
            Self::WorkflowDefinition(_) => AUTONOMOUS_TOOL_WORKFLOW_DEFINITION,
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
    if let AutonomousBrowserAction::InAppCdpFacade { method, .. } = action {
        return if in_app_cdp_facade_method_is_observe_tool(method) {
            AUTONOMOUS_TOOL_BROWSER_OBSERVE
        } else {
            AUTONOMOUS_TOOL_BROWSER_CONTROL
        };
    }
    if let AutonomousBrowserAction::ActionCache { command, .. } = action {
        return if matches!(command.as_str(), "stats" | "list" | "get") {
            AUTONOMOUS_TOOL_BROWSER_OBSERVE
        } else {
            AUTONOMOUS_TOOL_BROWSER_CONTROL
        };
    }
    match action {
        AutonomousBrowserAction::Launch { .. }
        | AutonomousBrowserAction::Attach { .. }
        | AutonomousBrowserAction::Close { .. }
        | AutonomousBrowserAction::Open { .. }
        | AutonomousBrowserAction::TabOpen { .. }
        | AutonomousBrowserAction::Navigate { .. }
        | AutonomousBrowserAction::Back
        | AutonomousBrowserAction::Forward
        | AutonomousBrowserAction::Reload
        | AutonomousBrowserAction::Stop
        | AutonomousBrowserAction::Click { .. }
        | AutonomousBrowserAction::Type { .. }
        | AutonomousBrowserAction::Scroll { .. }
        | AutonomousBrowserAction::Hover { .. }
        | AutonomousBrowserAction::PressKey { .. }
        | AutonomousBrowserAction::ClickRef { .. }
        | AutonomousBrowserAction::FillRef { .. }
        | AutonomousBrowserAction::HoverRef { .. }
        | AutonomousBrowserAction::SelectOption { .. }
        | AutonomousBrowserAction::SetChecked { .. }
        | AutonomousBrowserAction::Drag { .. }
        | AutonomousBrowserAction::UploadFile { .. }
        | AutonomousBrowserAction::Focus { .. }
        | AutonomousBrowserAction::Paste { .. }
        | AutonomousBrowserAction::SetViewport { .. }
        | AutonomousBrowserAction::ZoomRegion { .. }
        | AutonomousBrowserAction::Batch { .. }
        | AutonomousBrowserAction::Act { .. }
        | AutonomousBrowserAction::FillForm { .. }
        | AutonomousBrowserAction::DebugBundle { .. }
        | AutonomousBrowserAction::ExportBundle { .. }
        | AutonomousBrowserAction::Annotation { .. }
        | AutonomousBrowserAction::Recording { .. }
        | AutonomousBrowserAction::DialogAccept { .. }
        | AutonomousBrowserAction::DialogDismiss { .. }
        | AutonomousBrowserAction::DialogRespond { .. }
        | AutonomousBrowserAction::DownloadSave { .. }
        | AutonomousBrowserAction::DownloadClear { .. }
        | AutonomousBrowserAction::TraceStart { .. }
        | AutonomousBrowserAction::TraceStop { .. }
        | AutonomousBrowserAction::TraceExport { .. }
        | AutonomousBrowserAction::VisualBaselineSave { .. }
        | AutonomousBrowserAction::VisualDiff { .. }
        | AutonomousBrowserAction::VisualBaselineDelete { .. }
        | AutonomousBrowserAction::EmulateDevice { .. }
        | AutonomousBrowserAction::ClearEmulation { .. }
        | AutonomousBrowserAction::SwitchPage { .. }
        | AutonomousBrowserAction::ClosePage { .. }
        | AutonomousBrowserAction::SelectFrame { .. }
        | AutonomousBrowserAction::HarExport { .. }
        | AutonomousBrowserAction::PdfExport { .. }
        | AutonomousBrowserAction::NetworkControl { .. }
        | AutonomousBrowserAction::VaultSave { .. }
        | AutonomousBrowserAction::VaultLogin { .. }
        | AutonomousBrowserAction::VaultDelete { .. }
        | AutonomousBrowserAction::AuthProfileSave { .. }
        | AutonomousBrowserAction::AuthProfileRestore { .. }
        | AutonomousBrowserAction::AuthProfileDelete { .. }
        | AutonomousBrowserAction::ViewerGoal { .. }
        | AutonomousBrowserAction::Takeover { .. }
        | AutonomousBrowserAction::ReleaseControl { .. }
        | AutonomousBrowserAction::Pause { .. }
        | AutonomousBrowserAction::Resume { .. }
        | AutonomousBrowserAction::Step { .. }
        | AutonomousBrowserAction::Abort { .. }
        | AutonomousBrowserAction::SensitiveOn { .. }
        | AutonomousBrowserAction::SensitiveOff { .. }
        | AutonomousBrowserAction::McpBridge { .. }
        | AutonomousBrowserAction::GenerateTest { .. }
        | AutonomousBrowserAction::CookiesSet { .. }
        | AutonomousBrowserAction::StorageWrite { .. }
        | AutonomousBrowserAction::StorageClear { .. }
        | AutonomousBrowserAction::StateRestore { .. }
        | AutonomousBrowserAction::TabClose { .. }
        | AutonomousBrowserAction::TabFocus { .. } => AUTONOMOUS_TOOL_BROWSER_CONTROL,
        AutonomousBrowserAction::Health
        | AutonomousBrowserAction::Capabilities { .. }
        | AutonomousBrowserAction::PageList { .. }
        | AutonomousBrowserAction::ReadText { .. }
        | AutonomousBrowserAction::Source { .. }
        | AutonomousBrowserAction::Query { .. }
        | AutonomousBrowserAction::Snapshot { .. }
        | AutonomousBrowserAction::GetRef { .. }
        | AutonomousBrowserAction::WaitForSelector { .. }
        | AutonomousBrowserAction::WaitForLoad { .. }
        | AutonomousBrowserAction::WaitFor { .. }
        | AutonomousBrowserAction::Assert { .. }
        | AutonomousBrowserAction::CurrentUrl
        | AutonomousBrowserAction::HistoryState
        | AutonomousBrowserAction::Screenshot
        | AutonomousBrowserAction::CookiesGet
        | AutonomousBrowserAction::StorageRead { .. }
        | AutonomousBrowserAction::ConsoleLogs { .. }
        | AutonomousBrowserAction::NetworkSummary { .. }
        | AutonomousBrowserAction::AccessibilityTree { .. }
        | AutonomousBrowserAction::StateSnapshot { .. }
        | AutonomousBrowserAction::FindBest { .. }
        | AutonomousBrowserAction::AnalyzeForm { .. }
        | AutonomousBrowserAction::FrameList { .. }
        | AutonomousBrowserAction::DialogList { .. }
        | AutonomousBrowserAction::DownloadList { .. }
        | AutonomousBrowserAction::TraceStatus { .. }
        | AutonomousBrowserAction::VisualBaselineList { .. }
        | AutonomousBrowserAction::EmulationState { .. }
        | AutonomousBrowserAction::Extract { .. }
        | AutonomousBrowserAction::FrameState { .. }
        | AutonomousBrowserAction::VaultList { .. }
        | AutonomousBrowserAction::AuthProfileList { .. }
        | AutonomousBrowserAction::ViewerState { .. }
        | AutonomousBrowserAction::BrowserResource { .. }
        | AutonomousBrowserAction::BrowserPrompt { .. }
        | AutonomousBrowserAction::ValidateBundle { .. }
        | AutonomousBrowserAction::Timeline { .. }
        | AutonomousBrowserAction::PromptInjectionScan { .. }
        | AutonomousBrowserAction::HarnessExtensionContract
        | AutonomousBrowserAction::TabList => AUTONOMOUS_TOOL_BROWSER_OBSERVE,
        AutonomousBrowserAction::InAppCdpFacade { .. }
        | AutonomousBrowserAction::ActionCache { .. } => unreachable!("handled above"),
    }
}

fn in_app_cdp_facade_method_is_observe_tool(method: &str) -> bool {
    matches!(
        method,
        "Page.lifecycle"
            | "DOM.snapshot"
            | "DOM.resolveRef"
            | "Log.entryAdded"
            | "Network.requestWillBeSent"
            | "Network.responseReceived"
            | "Network.summary"
            | "Accessibility.snapshot"
            | "Storage.get"
    )
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
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub around_pattern: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes_per_file: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<usize>,
    #[serde(default)]
    pub include_line_hashes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadManyRequest {
    pub paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<AutonomousReadMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes_per_file: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_total_bytes: Option<usize>,
    #[serde(default)]
    pub include_line_hashes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousResultPageRequest {
    pub artifact_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_offset: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousStatRequest {
    pub path: String,
    #[serde(default)]
    pub follow_symlinks: bool,
    #[serde(default)]
    pub include_git_status: bool,
    #[serde(default)]
    pub include_hash: bool,
    #[serde(default)]
    pub strict: bool,
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
    #[serde(default)]
    pub files_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
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
    pub mode: Option<AutonomousFindMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
    #[serde(default)]
    pub include_hidden: bool,
    #[serde(default)]
    pub include_ignored: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
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
    #[serde(default)]
    pub preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousWriteRequest {
    pub path: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    #[serde(default)]
    pub create_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwrite: Option<bool>,
    #[serde(default)]
    pub preview: bool,
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
pub struct AutonomousCopyRequest {
    pub from: String,
    pub to: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwrite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_target_hash: Option<String>,
    #[serde(default)]
    pub preview: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousFsTransactionAction {
    #[default]
    CreateFile,
    ReplaceFile,
    EditFile,
    DeleteFile,
    DeleteDirectory,
    Rename,
    Copy,
    Mkdir,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFsTransactionRequest {
    pub operations: Vec<AutonomousFsTransactionOperation>,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub stop_on_first_error: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFsTransactionOperation {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub action: AutonomousFsTransactionAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replacement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>,
    #[serde(default)]
    pub replace_all: bool,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_source_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_target_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwrite: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parents: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exist_ok: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousStructuredEditFormat {
    Json,
    Toml,
    Yaml,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousStructuredEditFormattingMode {
    #[default]
    Normalize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousStructuredEditAction {
    Set,
    Delete,
    AppendUnique,
    SortKeys,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousStructuredEditRequest {
    pub path: String,
    pub operations: Vec<AutonomousStructuredEditOperation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    #[serde(default)]
    pub formatting_mode: AutonomousStructuredEditFormattingMode,
    #[serde(default)]
    pub preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousStructuredEditOperation {
    pub action: AutonomousStructuredEditAction,
    pub pointer: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDeleteRequest {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_digest: Option<String>,
    #[serde(default)]
    pub preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRenameRequest {
    pub from_path: String,
    pub to_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_target_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overwrite: Option<bool>,
    #[serde(default)]
    pub preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMkdirRequest {
    pub path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parents: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exist_ok: Option<bool>,
    #[serde(default)]
    pub preview: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_by: Option<AutonomousListSortBy>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sort_direction: Option<AutonomousListSortDirection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousListSortBy {
    Path,
    Name,
    Kind,
    Size,
    Modified,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousListSortDirection {
    Asc,
    Desc,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListTreeRequest {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_depth: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_entries: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_globs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_globs: Vec<String>,
    #[serde(default)]
    pub include_git_status: bool,
    #[serde(default)]
    pub show_omitted: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousDirectoryDigestHashMode {
    MetadataOnly,
    ContentHash,
    GitIndexAware,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDirectoryDigestRequest {
    pub path: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_globs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_globs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_files: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_mode: Option<AutonomousDirectoryDigestHashMode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHashRequest {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub include_globs: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_globs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_files: Option<usize>,
    #[serde(default)]
    pub manifest: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandRequest {
    #[serde(deserialize_with = "deserialize_command_argv")]
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandSessionStartRequest {
    #[serde(deserialize_with = "deserialize_command_argv")]
    pub argv: Vec<String>,
    pub cwd: Option<String>,
    pub timeout_ms: Option<u64>,
}

fn deserialize_command_argv<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = JsonValue::deserialize(deserializer)?;
    command_argv_from_json_value(value).map_err(de::Error::custom)
}

fn command_argv_from_json_value(value: JsonValue) -> Result<Vec<String>, String> {
    match value {
        JsonValue::Array(items) => command_argv_from_json_array(items),
        JsonValue::String(raw) => command_argv_from_string(&raw),
        other => Err(format!(
            "argv must be array or string, got {}",
            json_value_type_name(&other)
        )),
    }
}

fn command_argv_from_json_array(items: Vec<JsonValue>) -> Result<Vec<String>, String> {
    items
        .into_iter()
        .enumerate()
        .map(|(index, item)| match item {
            JsonValue::String(value) => Ok(value),
            other => Err(format!(
                "argv[{index}] must be string, got {}",
                json_value_type_name(&other)
            )),
        })
        .collect()
}

fn command_argv_from_string(raw: &str) -> Result<Vec<String>, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("argv string cannot be empty".into());
    }

    if trimmed.starts_with('[') {
        let json_array = command_argv_json_array_prefix(trimmed).unwrap_or(trimmed);
        return serde_json::from_str::<Vec<String>>(json_array).map_err(|error| {
            format!("argv string must contain a JSON string array or plain argv tokens: {error}")
        });
    }

    if trimmed.chars().any(|character| {
        matches!(
            character,
            '\n' | '\r'
                | '"'
                | '\''
                | '\\'
                | '|'
                | '&'
                | ';'
                | '<'
                | '>'
                | '`'
                | '$'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
        )
    }) {
        return Err("argv string contains shell syntax; pass argv as a JSON array instead".into());
    }

    Ok(trimmed
        .split_whitespace()
        .map(str::to_owned)
        .collect::<Vec<_>>())
}

fn command_argv_json_array_prefix(input: &str) -> Option<&str> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;

    for (index, character) in input.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if in_string {
            match character {
                '\\' => escaped = true,
                '"' => in_string = false,
                _ => {}
            }
            continue;
        }

        match character {
            '"' => in_string = true,
            '[' => depth += 1,
            ']' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(&input[..index + character.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

fn json_value_type_name(value: &JsonValue) -> &'static str {
    match value {
        JsonValue::Null => "null",
        JsonValue::Bool(_) => "boolean",
        JsonValue::Number(_) => "number",
        JsonValue::String(_) => "string",
        JsonValue::Array(_) => "array",
        JsonValue::Object(_) => "object",
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHostCommandRequest {
    pub argv: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    #[serde(default)]
    pub preview: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_token: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rollback_hints: Vec<String>,
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
pub enum AutonomousRuntimeWaitKind {
    Sleep,
    ProcessExit,
    ProcessReady,
    ProcessOutput,
}

impl AutonomousRuntimeWaitKind {
    const fn as_wakeup_kind(self) -> project_store::AgentRunWakeupKind {
        match self {
            Self::Sleep => project_store::AgentRunWakeupKind::Sleep,
            Self::ProcessExit => project_store::AgentRunWakeupKind::ProcessExit,
            Self::ProcessReady => project_store::AgentRunWakeupKind::ProcessReady,
            Self::ProcessOutput => project_store::AgentRunWakeupKind::ProcessOutput,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRuntimeWaitRequest {
    pub kind: AutonomousRuntimeWaitKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_pattern: Option<String>,
    pub reason: String,
    #[serde(default = "default_runtime_wait_resume_context")]
    pub resume_context: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRuntimeWaitOutput {
    pub wake_id: String,
    pub kind: AutonomousRuntimeWaitKind,
    pub status: String,
    pub due_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deadline_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poll_interval_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_id: Option<String>,
    pub reason: String,
    pub resume_context: JsonValue,
    pub message: String,
}

fn default_runtime_wait_resume_context() -> JsonValue {
    JsonValue::Object(Default::default())
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
            Self::Emulator => "Return device state, reproduction steps, and automation evidence.",
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
    let workflow_structure_json = task
        .workflow_structure
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_subagent_stage_serialize_failed",
                format!("Xero could not serialize child Stage state: {error}"),
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
        workflow_structure_json,
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
    let workflow_structure = record
        .workflow_structure_json
        .as_deref()
        .map(serde_json::from_str::<JsonValue>)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_subagent_stage_decode_failed",
                format!("Xero could not decode durable child Stage state: {error}"),
            )
        })?;
    Ok(AutonomousSubagentTask {
        subagent_id: record.subagent_id,
        role,
        role_label: record.role_label,
        prompt: record.prompt_preview,
        model_id: record.model_id,
        write_set,
        workflow_structure,
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
    pub workflow_structure: Option<JsonValue>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_structure: Option<JsonValue>,
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
    pub expected_hash: Option<String>,
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
pub enum AutonomousToolOutput {
    Read(AutonomousReadOutput),
    ReadMany(AutonomousReadManyOutput),
    ResultPage(AutonomousResultPageOutput),
    Stat(AutonomousStatOutput),
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
    Copy(AutonomousCopyOutput),
    FsTransaction(AutonomousFsTransactionOutput),
    JsonEdit(AutonomousStructuredEditOutput),
    TomlEdit(AutonomousStructuredEditOutput),
    YamlEdit(AutonomousStructuredEditOutput),
    Delete(AutonomousDeleteOutput),
    Rename(AutonomousRenameOutput),
    Mkdir(AutonomousMkdirOutput),
    List(AutonomousListOutput),
    ListTree(AutonomousListTreeOutput),
    DirectoryDigest(AutonomousDirectoryDigestOutput),
    Hash(AutonomousHashOutput),
    Command(AutonomousCommandOutput),
    CommandSession(AutonomousCommandSessionOutput),
    ProcessManager(AutonomousProcessManagerOutput),
    RuntimeWait(AutonomousRuntimeWaitOutput),
    ActionRequired(AutonomousActionRequiredOutput),
    RouteRequest(AutonomousRouteRequestOutput),
    SystemDiagnostics(AutonomousSystemDiagnosticsOutput),
    MacosAutomation(AutonomousMacosAutomationOutput),
    DesktopObserve(AutonomousDesktopToolOutput),
    DesktopControl(AutonomousDesktopToolOutput),
    DesktopStream(AutonomousDesktopToolOutput),
    Mcp(AutonomousMcpOutput),
    Subagent(AutonomousSubagentOutput),
    SensitiveInput(AutonomousSensitiveInputOutput),
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
    WorkflowDefinition(AutonomousWorkflowDefinitionOutput),
    Skill(AutonomousSkillToolOutput),
    Browser(AutonomousBrowserOutput),
    Emulator(AutonomousEmulatorOutput),
    Solana(AutonomousSolanaOutput),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadOutput {
    pub path: String,
    pub path_kind: AutonomousStatKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    pub start_line: usize,
    pub line_count: usize,
    pub total_lines: usize,
    pub truncated: bool,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_omitted_reason: Option<String>,
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
pub struct AutonomousReadManyError {
    pub code: String,
    pub class: CommandErrorClass,
    pub message: String,
    pub retryable: bool,
}

impl From<CommandError> for AutonomousReadManyError {
    fn from(error: CommandError) -> Self {
        Self {
            code: error.code,
            class: error.class,
            message: error.message,
            retryable: error.retryable,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadManyItem {
    pub path: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read: Option<AutonomousReadOutput>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AutonomousReadManyError>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub omitted_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousReadManyOutput {
    pub paths: Vec<String>,
    pub results: Vec<AutonomousReadManyItem>,
    pub total_files: usize,
    pub ok_files: usize,
    pub error_files: usize,
    pub omitted_files: usize,
    pub total_bytes: u64,
    pub omitted_bytes: u64,
    pub truncated: bool,
    pub max_bytes_per_file: usize,
    pub max_total_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousResultPageOutput {
    pub artifact_path: String,
    pub byte_offset: u64,
    pub byte_count: usize,
    pub total_bytes: u64,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_byte_offset: Option<u64>,
    pub content: String,
    pub encoding: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousStatKind {
    File,
    Directory,
    Symlink,
    Missing,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousStatPermissions {
    pub readonly: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unix_mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousStatOutput {
    pub path: String,
    pub path_kind: AutonomousStatKind,
    pub exists: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permissions: Option<AutonomousStatPermissions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash_omitted_reason: Option<String>,
    #[serde(default)]
    pub follow_symlinks: bool,
    #[serde(default)]
    pub include_git_status: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub git_status: Vec<RepositoryStatusEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchOutput {
    pub query: String,
    pub scope: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<AutonomousSearchFileSummary>,
    pub matches: Vec<AutonomousSearchMatch>,
    pub scanned_files: usize,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub files_only: bool,
    #[serde(default)]
    pub returned_matches: usize,
    #[serde(default)]
    pub skipped_matches: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_matches: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub matched_files: Option<usize>,
    pub omissions: AutonomousSearchOmissions,
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
pub struct AutonomousSearchFileSummary {
    pub path: String,
    pub match_count: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_line: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_preview: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchOmissions {
    #[serde(default)]
    pub ignored_directories: usize,
    #[serde(default)]
    pub filtered_files: usize,
    #[serde(default)]
    pub binary_files: usize,
    #[serde(default)]
    pub oversized_files: usize,
    #[serde(default)]
    pub unreadable_files: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFindOutput {
    pub pattern: String,
    pub scope: Option<String>,
    pub mode: AutonomousFindMode,
    pub matches: Vec<String>,
    pub scanned_files: usize,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    #[serde(default)]
    pub returned_matches: usize,
    #[serde(default)]
    pub skipped_matches: usize,
    #[serde(default)]
    pub file_count: usize,
    #[serde(default)]
    pub directory_count: usize,
    #[serde(default)]
    pub symlink_count: usize,
    #[serde(default)]
    pub other_count: usize,
    pub omissions: AutonomousFindOmissions,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousFindMode {
    Glob,
    Name,
    Extension,
    PathPrefix,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFindOmissions {
    #[serde(default)]
    pub ignored_directories: usize,
    #[serde(default)]
    pub depth_limited_directories: usize,
    #[serde(default)]
    pub permission_denied: usize,
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_summaries: Vec<AutonomousToolAccessToolSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolAccessToolSummary {
    pub tool_name: String,
    pub effect_class: String,
    pub risk_class: String,
    pub runtime_available: bool,
    pub allowed_for_agent: bool,
    pub activation_groups: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolAccessOutput {
    pub action: String,
    pub granted_tools: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub granted_tool_details: Vec<AutonomousToolAccessToolSummary>,
    pub denied_tools: Vec<String>,
    pub available_groups: Vec<AutonomousToolAccessGroup>,
    #[serde(default)]
    pub available_tool_packs: Vec<DomainToolPackManifest>,
    #[serde(default)]
    pub tool_pack_health: Vec<DomainToolPackHealthReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability_manifest: Option<JsonValue>,
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
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
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
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_bytes: Option<usize>,
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
    pub rollback_status: AutonomousFsTransactionRollbackStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(default)]
    pub diff_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
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
    pub guard_status: AutonomousPatchGuardStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_ranges: Vec<AutonomousPatchChangedRange>,
    pub line_ending: AutonomousLineEnding,
    pub bom_preserved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPatchGuardStatus {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub expected_hashes: Vec<String>,
    pub current_hash: String,
    pub matched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousPatchChangedRange {
    pub start_line: usize,
    pub end_line: usize,
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
pub struct AutonomousCopyOmissions {
    pub symlinks: usize,
    pub existing_targets: usize,
    pub unsupported: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCopyOperation {
    pub action: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from_path: Option<String>,
    pub to_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(default)]
    pub overwritten: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCopyOutput {
    pub from_path: String,
    pub to_path: String,
    pub recursive: bool,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub overwritten: bool,
    pub copied_files: usize,
    pub copied_bytes: u64,
    pub created_directories: usize,
    pub source_kind: AutonomousStatKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_hash: Option<String>,
    pub omitted: AutonomousCopyOmissions,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub operations: Vec<AutonomousCopyOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFsTransactionError {
    pub code: String,
    pub class: CommandErrorClass,
    pub message: String,
    pub retryable: bool,
}

impl From<CommandError> for AutonomousFsTransactionError {
    fn from(error: CommandError) -> Self {
        Self {
            code: error.code,
            class: error.class,
            message: error.message,
            retryable: error.retryable,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFsTransactionValidationSummary {
    pub ok: bool,
    pub validated_operations: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub errors: Vec<AutonomousFsTransactionOperationResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFsTransactionRollbackAttempt {
    pub path: String,
    pub action: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AutonomousFsTransactionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFsTransactionRollbackStatus {
    pub attempted: bool,
    pub succeeded: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attempts: Vec<AutonomousFsTransactionRollbackAttempt>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFsTransactionOperationResult {
    pub index: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub action: AutonomousFsTransactionAction,
    pub ok: bool,
    pub status: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AutonomousFsTransactionError>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousFsTransactionOutput {
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
    pub operation_count: usize,
    pub validation: AutonomousFsTransactionValidationSummary,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_paths: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub planned_operations: Vec<AutonomousFsTransactionOperationResult>,
    pub rollback_status: AutonomousFsTransactionRollbackStatus,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub results: Vec<AutonomousFsTransactionOperationResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousStructuredEditOutput {
    pub path: String,
    pub format: AutonomousStructuredEditFormat,
    pub operations_applied: usize,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
    pub formatting_mode: AutonomousStructuredEditFormattingMode,
    pub old_hash: String,
    pub new_hash: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub diff: Option<String>,
    pub line_ending: AutonomousLineEnding,
    pub bom_preserved: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub semantic_changes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDeleteOutput {
    pub path: String,
    pub recursive: bool,
    pub existed: bool,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub deleted_count: usize,
    #[serde(default)]
    pub file_count: usize,
    #[serde(default)]
    pub directory_count: usize,
    #[serde(default)]
    pub symlink_count: usize,
    #[serde(default)]
    pub other_count: usize,
    #[serde(default)]
    pub bytes_estimated: u64,
    #[serde(default)]
    pub bytes_remaining: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousRenameOutput {
    pub from_path: String,
    pub to_path: String,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
    #[serde(default)]
    pub overwritten: bool,
    pub source_kind: AutonomousStatKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_hash: Option<String>,
    #[serde(default)]
    pub target_existed: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_kind: Option<AutonomousStatKind>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMkdirOutput {
    pub path: String,
    pub created: bool,
    #[serde(default)]
    pub applied: bool,
    #[serde(default)]
    pub preview: bool,
    pub parents: bool,
    pub exist_ok: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub created_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListEntry {
    pub path: String,
    pub kind: String,
    pub bytes: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListOutput {
    pub path: String,
    pub entries: Vec<AutonomousListEntry>,
    pub truncated: bool,
    pub max_depth: usize,
    pub max_results: usize,
    pub sort_by: AutonomousListSortBy,
    pub sort_direction: AutonomousListSortDirection,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub returned_entries: usize,
    pub skipped_entries: usize,
    pub file_count: usize,
    pub directory_count: usize,
    pub symlink_count: usize,
    pub other_count: usize,
    pub omitted: AutonomousListOmissions,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListOmissions {
    pub depth: usize,
    pub entry_cap: usize,
    pub ignored_directory: usize,
    pub permission: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListTreeOmissions {
    pub depth: usize,
    pub entry_cap: usize,
    pub ignored_directory: usize,
    pub permission: usize,
    pub filtered: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListTreeNode {
    pub name: String,
    pub path: String,
    pub path_kind: AutonomousStatKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<AutonomousListTreeNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousListTreeOutput {
    pub path: String,
    pub root: AutonomousListTreeNode,
    pub file_count: usize,
    pub directory_count: usize,
    pub symlink_count: usize,
    pub other_count: usize,
    pub max_depth: usize,
    pub max_entries: usize,
    pub truncated: bool,
    pub omitted: AutonomousListTreeOmissions,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub git_status: Vec<RepositoryStatusEntryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDirectoryDigestOmissions {
    pub max_files: usize,
    pub ignored_directory: usize,
    pub permission: usize,
    pub filtered: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDirectoryDigestEntry {
    pub path: String,
    pub path_kind: AutonomousStatKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousDirectoryDigestOutput {
    pub path: String,
    pub digest: String,
    pub algorithm: String,
    pub hash_mode: AutonomousDirectoryDigestHashMode,
    pub file_count: usize,
    pub directory_count: usize,
    pub symlink_count: usize,
    pub other_count: usize,
    pub total_bytes: u64,
    pub max_files: usize,
    pub truncated: bool,
    pub omitted: AutonomousDirectoryDigestOmissions,
    pub manifest: Vec<AutonomousDirectoryDigestEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHashOutput {
    pub path: String,
    pub path_kind: AutonomousStatKind,
    pub algorithm: String,
    pub mode: String,
    pub sha256: String,
    pub bytes: u64,
    pub file_count: usize,
    pub max_files: usize,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<AutonomousHashFileEntry>,
    pub omitted: AutonomousHashOmissions,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHashFileEntry {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHashOmissions {
    pub max_files: usize,
    pub ignored_directory: usize,
    pub permission: usize,
    pub filtered: usize,
    pub unsupported: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandOutput {
    pub argv: Vec<String>,
    pub cwd: String,
    pub intent: String,
    pub stdout: Option<String>,
    pub stderr: Option<String>,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub stdout_redacted: bool,
    pub stderr_redacted: bool,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub spawned: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_token: Option<String>,
    pub policy: AutonomousCommandPolicyTrace,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed_files: Vec<RepositoryStatusEntryDto>,
    pub changed_files_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_artifact: Option<AutonomousCommandOutputArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suggested_next_actions: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_command_impact: Option<AutonomousHostCommandImpact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<SandboxExecutionMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHostCommandImpact {
    pub schema: String,
    pub policy_profile: AutonomousCommandPolicyProfile,
    pub requires_preview: bool,
    pub requires_owner_approval: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preview_token_validated: Option<bool>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub detected_surfaces: Vec<AutonomousHostCommandImpactSurface>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rollback_hints: Vec<String>,
    pub elevation: AutonomousHostCommandElevationAssessment,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_admin_expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHostCommandImpactSurface {
    pub category: String,
    pub evidence: String,
    pub impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousHostCommandElevationAssessment {
    pub uses_os_native_prompt: bool,
    pub bypasses_os_protection: bool,
    pub protected_boundaries: Vec<String>,
    pub user_action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousCommandOutputArtifact {
    pub path: String,
    pub byte_count: usize,
    pub stdout_bytes: usize,
    pub stderr_bytes: usize,
    pub redacted: bool,
    pub truncated: bool,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_artifact: Option<AutonomousMcpResultArtifact>,
    #[serde(default)]
    pub result_truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_original_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousMcpResultArtifact {
    pub id: String,
    pub path: String,
    pub byte_count: usize,
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
    pub old_hash: String,
    pub new_hash: String,
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
    pub effect_class: String,
    pub runtime_available: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub why_matched: Vec<String>,
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
    ToolExtension {
        extension_id: String,
        installation_hash: String,
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

    use crate::commands::{
        solana::{
            AnalyzerKind, ClusterKind, CodamaTarget, Commitment, DeployAuthority, ExplainRequest,
            IdlPublishMode, SeedPart, SendRequest, SimulateRequest, TxSpec,
        },
        RuntimeAgentIdDto,
    };

    #[test]
    fn browser_gap_actions_route_to_observe_or_control_tool_names() {
        for action in [
            AutonomousBrowserAction::DialogList { session_id: None },
            AutonomousBrowserAction::DownloadList { session_id: None },
            AutonomousBrowserAction::TraceStatus { session_id: None },
            AutonomousBrowserAction::Extract {
                session_id: None,
                mode: "links".into(),
                selector: None,
                selector_map: None,
                limit: None,
            },
            AutonomousBrowserAction::ViewerState { session_id: None },
            AutonomousBrowserAction::BrowserPrompt {
                prompt: "full_page_audit".into(),
                arguments: None,
            },
            AutonomousBrowserAction::ActionCache {
                command: "stats".into(),
                scope: None,
                url_signature: None,
                intent: None,
                key: None,
                selector_candidates: None,
                confidence: None,
            },
            AutonomousBrowserAction::InAppCdpFacade {
                method: "DOM.snapshot".into(),
                params: None,
                timeout_ms: None,
            },
        ] {
            let request = AutonomousToolRequest::Browser(AutonomousBrowserRequest { action });
            assert_eq!(request.tool_name(), AUTONOMOUS_TOOL_BROWSER_OBSERVE);
        }

        for action in [
            AutonomousBrowserAction::SelectOption {
                selector: Some("select".into()),
                ref_id: None,
                value: None,
                label: Some("One".into()),
                index: None,
                timeout_ms: None,
            },
            AutonomousBrowserAction::DialogAccept {
                session_id: None,
                prompt_text: None,
            },
            AutonomousBrowserAction::DownloadSave {
                session_id: None,
                guid: "download-1".into(),
                destination: "/tmp/download.txt".into(),
            },
            AutonomousBrowserAction::VisualDiff {
                session_id: None,
                name: "baseline".into(),
                threshold_percent: None,
                selector: None,
                ref_id: None,
                full_page: None,
            },
            AutonomousBrowserAction::AuthProfileRestore {
                session_id: None,
                name: "profile".into(),
                navigate: None,
            },
            AutonomousBrowserAction::GenerateTest {
                recording_id: None,
                batch_json: Some("{}".into()),
                name: None,
            },
            AutonomousBrowserAction::ActionCache {
                command: "put".into(),
                scope: None,
                url_signature: Some("https://example.com/#Example".into()),
                intent: Some("click cta".into()),
                key: None,
                selector_candidates: Some(vec!["#cta".into()]),
                confidence: Some(90),
            },
            AutonomousBrowserAction::InAppCdpFacade {
                method: "Input.click".into(),
                params: None,
                timeout_ms: None,
            },
        ] {
            let request = AutonomousToolRequest::Browser(AutonomousBrowserRequest { action });
            assert_eq!(request.tool_name(), AUTONOMOUS_TOOL_BROWSER_CONTROL);
        }
    }

    #[test]
    fn runtime_wait_validation_enforces_bounded_safe_waits() {
        validate_runtime_wait_request(&AutonomousRuntimeWaitRequest {
            kind: AutonomousRuntimeWaitKind::Sleep,
            delay_ms: Some(MIN_RUNTIME_WAIT_DELAY_MS),
            process_id: None,
            poll_interval_ms: None,
            deadline_ms: None,
            output_pattern: None,
            reason: "Pause briefly before checking again.".into(),
            resume_context: json!({ "nextStep": "inspect status" }),
        })
        .expect("bounded sleep wait is valid");

        let missing_deadline = validate_runtime_wait_request(&AutonomousRuntimeWaitRequest {
            kind: AutonomousRuntimeWaitKind::ProcessExit,
            delay_ms: None,
            process_id: Some("proc-1".into()),
            poll_interval_ms: Some(DEFAULT_RUNTIME_WAIT_POLL_INTERVAL_MS),
            deadline_ms: None,
            output_pattern: None,
            reason: "Resume when the process exits.".into(),
            resume_context: json!({}),
        })
        .expect_err("process waits require a deadline");
        assert_eq!(missing_deadline.code, "runtime_wait_deadline_required");

        let invalid_regex = validate_runtime_wait_request(&AutonomousRuntimeWaitRequest {
            kind: AutonomousRuntimeWaitKind::ProcessOutput,
            delay_ms: None,
            process_id: Some("proc-1".into()),
            poll_interval_ms: Some(DEFAULT_RUNTIME_WAIT_POLL_INTERVAL_MS),
            deadline_ms: Some(DEFAULT_RUNTIME_WAIT_POLL_INTERVAL_MS * 3),
            output_pattern: Some("[".into()),
            reason: "Resume when build output is visible.".into(),
            resume_context: json!({}),
        })
        .expect_err("process output waits require a valid regex");
        assert_eq!(invalid_regex.code, "runtime_wait_output_pattern_invalid");

        let secret_reason = validate_runtime_wait_request(&AutonomousRuntimeWaitRequest {
            kind: AutonomousRuntimeWaitKind::Sleep,
            delay_ms: Some(MIN_RUNTIME_WAIT_DELAY_MS),
            process_id: None,
            poll_interval_ms: None,
            deadline_ms: None,
            output_pattern: None,
            reason: "api_key=sk-test-secret".into(),
            resume_context: json!({}),
        })
        .expect_err("reasons must not persist secret-like text");
        assert_eq!(secret_reason.code, "runtime_wait_reason_secret_like");
    }

    #[test]
    fn action_required_validation_enforces_bounded_safe_prompts() {
        validate_action_required_request(&AutonomousActionRequiredRequest {
            title: "Choose a stack".into(),
            detail: "Select the technology stack before implementation starts.".into(),
            answer_shape: AutonomousActionRequiredAnswerShape::SingleChoice,
            prompt_kind: Some("technology_stack_selection".into()),
            options: vec![
                AutonomousActionRequiredOption {
                    id: "existing".into(),
                    label: "Existing stack".into(),
                    description: Some("Follow the current project conventions.".into()),
                },
                AutonomousActionRequiredOption {
                    id: "react-vite".into(),
                    label: "React + Vite".into(),
                    description: None,
                },
            ],
            intended_use: Some("Use the selected stack for the implementation plan.".into()),
        })
        .expect("valid bounded choice prompt");

        let missing_options = validate_action_required_request(&AutonomousActionRequiredRequest {
            title: "Choose a stack".into(),
            detail: "Select the technology stack before implementation starts.".into(),
            answer_shape: AutonomousActionRequiredAnswerShape::SingleChoice,
            prompt_kind: None,
            options: Vec::new(),
            intended_use: None,
        })
        .expect_err("choice prompts require options");
        assert_eq!(missing_options.code, "action_required_options_invalid");

        let unexpected_options =
            validate_action_required_request(&AutonomousActionRequiredRequest {
                title: "Name it".into(),
                detail: "Provide a short display name for this plan.".into(),
                answer_shape: AutonomousActionRequiredAnswerShape::ShortText,
                prompt_kind: None,
                options: vec![AutonomousActionRequiredOption {
                    id: "one".into(),
                    label: "One".into(),
                    description: None,
                }],
                intended_use: None,
            })
            .expect_err("text prompts cannot include options");
        assert_eq!(
            unexpected_options.code,
            "action_required_options_unexpected"
        );

        let secret_detail = validate_action_required_request(&AutonomousActionRequiredRequest {
            title: "Choose a stack".into(),
            detail: "Use api_key=sk-test-secret for this selection.".into(),
            answer_shape: AutonomousActionRequiredAnswerShape::ShortText,
            prompt_kind: None,
            options: Vec::new(),
            intended_use: None,
        })
        .expect_err("prompt metadata must not contain secrets");
        assert_eq!(secret_detail.code, "action_required_metadata_secret_like");
    }

    #[test]
    fn route_request_validation_rejects_malformed_and_secret_like_routes() {
        validate_route_request(&AutonomousRouteRequest {
            target_kind: AutonomousRouteTargetKind::BuiltIn,
            target_agent_id: Some(RuntimeAgentIdDto::Engineer),
            target_agent_definition_id: None,
            target_agent_definition_version: None,
            reason: "Implementation is the next useful step.".into(),
            summary: "Carry the approved plan into implementation.".into(),
        })
        .expect("valid built-in route request");

        let missing_runtime_id = validate_route_request(&AutonomousRouteRequest {
            target_kind: AutonomousRouteTargetKind::BuiltIn,
            target_agent_id: None,
            target_agent_definition_id: None,
            target_agent_definition_version: None,
            reason: "Implementation is the next useful step.".into(),
            summary: "Carry the approved plan into implementation.".into(),
        })
        .expect_err("built-in routes require a runtime id");
        assert_eq!(missing_runtime_id.code, "agent_route_target_id_required");

        let mixed_identity = validate_route_request(&AutonomousRouteRequest {
            target_kind: AutonomousRouteTargetKind::BuiltIn,
            target_agent_id: Some(RuntimeAgentIdDto::Engineer),
            target_agent_definition_id: Some("custom-engineer".into()),
            target_agent_definition_version: Some(3),
            reason: "Implementation is the next useful step.".into(),
            summary: "Carry the approved plan into implementation.".into(),
        })
        .expect_err("built-in routes cannot smuggle custom identity");
        assert_eq!(
            mixed_identity.code,
            "agent_route_builtin_definition_forbidden"
        );

        let missing_definition_id = validate_route_request(&AutonomousRouteRequest {
            target_kind: AutonomousRouteTargetKind::Custom,
            target_agent_id: None,
            target_agent_definition_id: Some("  ".into()),
            target_agent_definition_version: None,
            reason: "A custom specialist is the next useful step.".into(),
            summary: "Carry the relevant context to the specialist.".into(),
        })
        .expect_err("custom routes require a definition id");
        assert_eq!(
            missing_definition_id.code,
            "agent_route_definition_id_required"
        );

        let secret_summary = validate_route_request(&AutonomousRouteRequest {
            target_kind: AutonomousRouteTargetKind::BuiltIn,
            target_agent_id: Some(RuntimeAgentIdDto::Engineer),
            target_agent_definition_id: None,
            target_agent_definition_version: None,
            reason: "Implementation is the next useful step.".into(),
            summary: "Use api_key=sk-test-secret during implementation.".into(),
        })
        .expect_err("persisted routes must reject secret-like content");
        assert_eq!(secret_summary.code, "agent_route_content_secret_like");
    }

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
            AUTONOMOUS_TOOL_HOST_COMMAND,
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
            AUTONOMOUS_TOOL_STAT,
            AUTONOMOUS_TOOL_SEARCH,
            AUTONOMOUS_TOOL_GIT_STATUS,
            AUTONOMOUS_TOOL_COMMAND_PROBE,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
            AUTONOMOUS_TOOL_WORKSPACE_INDEX,
            AUTONOMOUS_TOOL_ACTION_REQUIRED,
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
            AUTONOMOUS_TOOL_ACTION_REQUIRED,
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
    fn web_catalog_entries_match_request_schema_fields() {
        let catalog = deferred_tool_catalog(true);
        let search = catalog
            .iter()
            .find(|entry| entry.tool_name == AUTONOMOUS_TOOL_WEB_SEARCH)
            .expect("web search catalog entry");
        let fetch = catalog
            .iter()
            .find(|entry| entry.tool_name == AUTONOMOUS_TOOL_WEB_FETCH)
            .expect("web fetch catalog entry");

        assert_eq!(search.schema_fields, &["query", "resultCount", "timeoutMs"]);
        assert_eq!(fetch.schema_fields, &["url", "maxChars", "timeoutMs"]);
    }

    #[test]
    fn domain_tool_pack_tools_are_cataloged_and_requestable_through_tool_access() {
        let catalog_tools = deferred_tool_catalog(true)
            .into_iter()
            .map(|entry| entry.tool_name)
            .collect::<BTreeSet<_>>();
        let access_groups = TOOL_ACCESS_GROUP_DEFINITIONS
            .iter()
            .map(|definition| definition.name)
            .collect::<BTreeSet<_>>();

        for manifest in domain_tool_pack_manifests() {
            for group in &manifest.tool_groups {
                assert!(
                    access_groups.contains(group.as_str()),
                    "pack `{}` declares unknown activation group `{group}`",
                    manifest.pack_id
                );
            }

            for tool in &manifest.tools {
                assert!(
                    catalog_tools.contains(tool.as_str()),
                    "pack `{}` declares `{tool}` but the tool is absent from the agent catalog",
                    manifest.pack_id
                );

                let activation_groups = tool_catalog_activation_groups(tool);
                assert!(
                    !activation_groups.is_empty(),
                    "pack `{}` tool `{tool}` has no tool_access activation group",
                    manifest.pack_id
                );
                assert!(
                    activation_groups
                        .iter()
                        .any(|group| manifest.tool_groups.contains(group)),
                    "pack `{}` tool `{tool}` activation groups {:?} do not overlap declared pack groups {:?}",
                    manifest.pack_id,
                    activation_groups,
                    manifest.tool_groups
                );

                let metadata = tool_catalog_metadata_for_tool(tool, true)
                    .expect("cataloged pack tool should expose metadata");
                let pack_ids = metadata["toolPackIds"]
                    .as_array()
                    .expect("tool metadata pack ids")
                    .iter()
                    .filter_map(JsonValue::as_str)
                    .collect::<BTreeSet<_>>();
                assert!(
                    pack_ids.contains(manifest.pack_id.as_str()),
                    "catalog metadata for `{tool}` should include declaring pack `{}`",
                    manifest.pack_id
                );
            }
        }
    }

    #[test]
    fn catalog_entries_have_policy_classification_and_activation_metadata() {
        let access_tools = tool_access_all_known_tools();
        let computer_use_default_tools = computer_use_default_tool_names();
        let mut seen = BTreeSet::new();

        for entry in deferred_tool_catalog(true) {
            let active_by_default = computer_use_default_tools.contains(entry.tool_name);
            assert!(
                seen.insert(entry.tool_name),
                "tool catalog declares duplicate tool `{}`",
                entry.tool_name
            );
            assert!(
                !entry.description.trim().is_empty(),
                "tool `{}` needs a prompt-visible description",
                entry.tool_name
            );
            assert!(
                !entry.tags.is_empty(),
                "tool `{}` needs searchable catalog tags",
                entry.tool_name
            );
            assert!(
                !entry.examples.is_empty(),
                "tool `{}` needs prompt-visible examples",
                entry.tool_name
            );
            assert_ne!(
                tool_effect_class(entry.tool_name),
                AutonomousToolEffectClass::Unknown,
                "tool `{}` needs an effect-class policy mapping",
                entry.tool_name
            );
            assert!(
                !allowed_runtime_agent_labels(entry.tool_name).is_empty(),
                "tool `{}` should be allowed for at least one runtime agent",
                entry.tool_name
            );
            assert!(
                !tool_catalog_activation_groups(entry.tool_name).is_empty() || active_by_default,
                "tool `{}` should have at least one activation group or be active by default",
                entry.tool_name
            );
            assert!(
                access_tools.contains(entry.tool_name) || active_by_default,
                "tool `{}` is cataloged but absent from tool_access groups and default activation",
                entry.tool_name
            );
        }
    }

    #[derive(Debug, Default)]
    struct FixtureSolanaExecutor {
        deny_mutations: bool,
        leak_sensitive_output: bool,
    }

    fn fixture_solana_output(
        action: &str,
        value: JsonValue,
    ) -> CommandResult<AutonomousSolanaOutput> {
        Ok(AutonomousSolanaOutput {
            action: action.into(),
            value_json: serde_json::to_string(&value).expect("fixture json"),
        })
    }

    fn fixture_policy_denied(message: &str) -> CommandResult<AutonomousSolanaOutput> {
        Err(CommandError::policy_denied(message))
    }

    impl FixtureSolanaExecutor {
        fn output(&self, action: &str) -> CommandResult<AutonomousSolanaOutput> {
            let value = if self.leak_sensitive_output {
                json!({
                    "ok": true,
                    "action": action,
                    "keypairPath": "/Users/alice/.config/solana/id.json",
                    "rpcUrl": "https://rpc.helius.example/?api-key=live-secret-token",
                    "providerCredentials": {
                        "apiKey": "live-secret-token",
                        "authorization": "Bearer live-secret-token"
                    },
                    "walletMaterial": "-----BEGIN PRIVATE KEY----- live-secret",
                    "screenshotBase64": "iVBORw0KGgoAAAANSUhEUgAA",
                    "exportedDiagnostics": [
                        {
                            "message": "failed with private_key=live-secret",
                            "rawPayload": "tool_payload with api_key=live-secret"
                        }
                    ],
                    "telemetryPayload": "rpc token live-secret"
                })
            } else {
                json!({
                    "ok": true,
                    "action": action,
                    "shape": {
                        "cluster": "devnet",
                        "items": []
                    }
                })
            };
            fixture_solana_output(action, value)
        }
    }

    impl SolanaExecutor for FixtureSolanaExecutor {
        fn cluster(
            &self,
            _request: AutonomousSolanaClusterRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("cluster_status")
        }

        fn logs(
            &self,
            _request: AutonomousSolanaLogsRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("logs_recent")
        }

        fn tx(&self, request: AutonomousSolanaTxRequest) -> CommandResult<AutonomousSolanaOutput> {
            if self.deny_mutations
                && matches!(request.action, AutonomousSolanaTxAction::Send { .. })
            {
                return fixture_policy_denied(
                    "Solana send is mutation-adjacent and requires explicit approval; signed transaction bytes are not echoed.",
                );
            }
            self.output("tx_build")
        }

        fn simulate(
            &self,
            _request: AutonomousSolanaSimulateRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("simulate")
        }

        fn explain(
            &self,
            _request: AutonomousSolanaExplainRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("explain")
        }

        fn alt(
            &self,
            _request: AutonomousSolanaAltRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("alt_resolve")
        }

        fn idl(
            &self,
            request: AutonomousSolanaIdlRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            if self.deny_mutations
                && matches!(request.action, AutonomousSolanaIdlAction::Publish { .. })
            {
                return fixture_policy_denied(
                    "Solana IDL publish is mutation-adjacent and requires explicit approval; authority material is not echoed.",
                );
            }
            self.output("idl_get")
        }

        fn codama(
            &self,
            _request: AutonomousSolanaCodamaRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("codama_generate")
        }

        fn pda(
            &self,
            _request: AutonomousSolanaPdaRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("pda_derive")
        }

        fn program(
            &self,
            request: AutonomousSolanaProgramRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            if self.deny_mutations
                && matches!(
                    request.action,
                    AutonomousSolanaProgramAction::Rollback { .. }
                )
            {
                return fixture_policy_denied(
                    "Solana rollback is mutation-adjacent and requires explicit approval; authority material is not echoed.",
                );
            }
            self.output("program_build")
        }

        fn deploy(
            &self,
            _request: AutonomousSolanaDeployRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            if self.deny_mutations {
                return fixture_policy_denied(
                    "Solana deploy is mutation-adjacent and requires explicit approval; keypair paths are not echoed.",
                );
            }
            self.output("deploy")
        }

        fn upgrade_check(
            &self,
            _request: AutonomousSolanaUpgradeCheckRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("upgrade_check")
        }

        fn squads(
            &self,
            _request: AutonomousSolanaSquadsRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("squads_proposal")
        }

        fn verified_build(
            &self,
            _request: AutonomousSolanaVerifiedBuildRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            if self.deny_mutations {
                return fixture_policy_denied(
                    "Solana verified-build submission is mutation-adjacent and requires explicit approval; provider credentials are not echoed.",
                );
            }
            self.output("verified_build")
        }

        fn audit(
            &self,
            request: AutonomousSolanaAuditRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            let action = match request.action {
                AutonomousSolanaAuditAction::Static { .. } => "audit_static",
                AutonomousSolanaAuditAction::External { .. } => "audit_external",
                AutonomousSolanaAuditAction::Fuzz { .. }
                | AutonomousSolanaAuditAction::FuzzScaffold { .. } => "audit_fuzz",
                AutonomousSolanaAuditAction::Coverage { .. } => "audit_coverage",
            };
            self.output(action)
        }

        fn indexer(
            &self,
            _request: AutonomousSolanaIndexerRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("indexer_run")
        }

        fn replay(
            &self,
            _request: AutonomousSolanaReplayRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("replay_list")
        }

        fn secrets(
            &self,
            _request: AutonomousSolanaSecretsRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("secrets_patterns")
        }

        fn drift(
            &self,
            _request: AutonomousSolanaDriftRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("drift_tracked")
        }

        fn cost(
            &self,
            _request: AutonomousSolanaCostRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("cost_snapshot")
        }

        fn docs(
            &self,
            _request: AutonomousSolanaDocsRequest,
        ) -> CommandResult<AutonomousSolanaOutput> {
            self.output("docs_catalog")
        }
    }

    fn valid_pubkey(byte: u8) -> String {
        bs58::encode([byte; 32]).into_string()
    }

    fn representative_solana_runtime_requests(
    ) -> Vec<(&'static str, &'static str, AutonomousToolRequest)> {
        let program_id = valid_pubkey(7);
        let authority = DeployAuthority::DirectKeypair {
            keypair_path: "/tmp/xero-fixtures/devnet-authority.json".into(),
        };
        vec![
            (
                AUTONOMOUS_TOOL_SOLANA_CLUSTER,
                "cluster_status",
                AutonomousToolRequest::SolanaCluster(AutonomousSolanaClusterRequest {
                    action: AutonomousSolanaClusterAction::Status,
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_LOGS,
                "logs_recent",
                AutonomousToolRequest::SolanaLogs(AutonomousSolanaLogsRequest {
                    action: AutonomousSolanaLogsAction::Recent {
                        cluster: ClusterKind::Devnet,
                        program_ids: vec![program_id.clone()],
                        last_n: Some(5),
                        rpc_url: Some("https://api.devnet.solana.com".into()),
                        cached_only: true,
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_TX,
                "tx_build",
                AutonomousToolRequest::SolanaTx(AutonomousSolanaTxRequest {
                    action: AutonomousSolanaTxAction::Build {
                        spec: TxSpec {
                            cluster: ClusterKind::Devnet,
                            fee_payer_persona: "devnet-fee-payer".into(),
                            signer_personas: vec![],
                            program_ids: vec![program_id.clone()],
                            addresses: vec![program_id.clone()],
                            alt_candidates: vec![],
                            rpc_url: Some("https://api.devnet.solana.com".into()),
                        },
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_SIMULATE,
                "simulate",
                AutonomousToolRequest::SolanaSimulate(AutonomousSolanaSimulateRequest {
                    request: SimulateRequest {
                        cluster: ClusterKind::Devnet,
                        transaction_base64: "AQ==".into(),
                        rpc_url: Some("https://api.devnet.solana.com".into()),
                        skip_replace_blockhash: false,
                        idl_errors: None,
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_EXPLAIN,
                "explain",
                AutonomousToolRequest::SolanaExplain(AutonomousSolanaExplainRequest {
                    request: ExplainRequest {
                        cluster: ClusterKind::Devnet,
                        signature: "5EYjH9xGvQG6b6yVgS5tW9nV4T8a9cY7vZcMqqbC1Zmx".into(),
                        rpc_url: Some("https://api.devnet.solana.com".into()),
                        idl_errors: None,
                        commitment: Commitment::Finalized,
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_ALT,
                "alt_resolve",
                AutonomousToolRequest::SolanaAlt(AutonomousSolanaAltRequest {
                    action: AutonomousSolanaAltAction::Resolve {
                        addresses: vec![program_id.clone()],
                        candidates: vec![],
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_IDL,
                "idl_get",
                AutonomousToolRequest::SolanaIdl(AutonomousSolanaIdlRequest {
                    action: AutonomousSolanaIdlAction::Get {
                        program_id: program_id.clone(),
                        cluster: Some(ClusterKind::Devnet),
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_CODAMA,
                "codama_generate",
                AutonomousToolRequest::SolanaCodama(AutonomousSolanaCodamaRequest {
                    idl_path: "/tmp/xero-fixtures/anchor-idl.json".into(),
                    targets: vec![CodamaTarget::Ts],
                    output_dir: "/tmp/xero-fixtures/codama".into(),
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_PDA,
                "pda_derive",
                AutonomousToolRequest::SolanaPda(AutonomousSolanaPdaRequest {
                    action: AutonomousSolanaPdaAction::Derive {
                        program_id: program_id.clone(),
                        seeds: vec![SeedPart::Utf8("vault".into())],
                        bump: None,
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_PROGRAM,
                "program_build",
                AutonomousToolRequest::SolanaProgram(AutonomousSolanaProgramRequest {
                    action: AutonomousSolanaProgramAction::Build {
                        manifest_path: "/tmp/xero-fixtures/Cargo.toml".into(),
                        profile: None,
                        kind: None,
                        program: Some("fixture_program".into()),
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_DEPLOY,
                "deploy",
                AutonomousToolRequest::SolanaDeploy(AutonomousSolanaDeployRequest {
                    program_id: program_id.clone(),
                    cluster: ClusterKind::Devnet,
                    so_path: "/tmp/xero-fixtures/fixture_program.so".into(),
                    authority: authority.clone(),
                    idl_path: None,
                    is_first_deploy: false,
                    post: None,
                    rpc_url: Some("https://api.devnet.solana.com".into()),
                    project_root: None,
                    block_on_any_secret: false,
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_UPGRADE_CHECK,
                "upgrade_check",
                AutonomousToolRequest::SolanaUpgradeCheck(AutonomousSolanaUpgradeCheckRequest {
                    program_id: program_id.clone(),
                    cluster: ClusterKind::Devnet,
                    local_so_path: "/tmp/xero-fixtures/fixture_program.so".into(),
                    expected_authority: valid_pubkey(8),
                    local_idl_path: None,
                    max_program_size_bytes: Some(4096),
                    local_so_size_bytes: Some(1024),
                    rpc_url: Some("https://api.devnet.solana.com".into()),
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_SQUADS,
                "squads_proposal",
                AutonomousToolRequest::SolanaSquads(AutonomousSolanaSquadsRequest {
                    program_id: program_id.clone(),
                    cluster: ClusterKind::Devnet,
                    multisig_pda: valid_pubkey(9),
                    buffer: valid_pubkey(10),
                    spill: valid_pubkey(11),
                    creator: valid_pubkey(12),
                    vault_index: Some(0),
                    memo: Some("fixture proposal".into()),
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_VERIFIED_BUILD,
                "verified_build",
                AutonomousToolRequest::SolanaVerifiedBuild(AutonomousSolanaVerifiedBuildRequest {
                    program_id: program_id.clone(),
                    cluster: ClusterKind::Devnet,
                    manifest_path: "/tmp/xero-fixtures/Cargo.toml".into(),
                    github_url: "https://github.com/hyperpush-org/xero".into(),
                    commit_hash: Some("0123456789abcdef".into()),
                    library_name: None,
                    skip_remote_submit: true,
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_AUDIT_STATIC,
                "audit_static",
                AutonomousToolRequest::SolanaAuditStatic(AutonomousSolanaAuditRequest {
                    action: AutonomousSolanaAuditAction::Static {
                        project_root: "/tmp/xero-fixtures".into(),
                        rule_ids: vec![],
                        skip_paths: vec![],
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_AUDIT_EXTERNAL,
                "audit_external",
                AutonomousToolRequest::SolanaAuditExternal(AutonomousSolanaAuditRequest {
                    action: AutonomousSolanaAuditAction::External {
                        project_root: "/tmp/xero-fixtures".into(),
                        analyzer: AnalyzerKind::Auto,
                        timeout_s: Some(5),
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_AUDIT_FUZZ,
                "audit_fuzz",
                AutonomousToolRequest::SolanaAuditFuzz(AutonomousSolanaAuditRequest {
                    action: AutonomousSolanaAuditAction::Fuzz {
                        project_root: "/tmp/xero-fixtures".into(),
                        target: "fixture_target".into(),
                        duration_s: Some(1),
                        corpus: None,
                        baseline_coverage_lines: None,
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_AUDIT_COVERAGE,
                "audit_coverage",
                AutonomousToolRequest::SolanaAuditCoverage(AutonomousSolanaAuditRequest {
                    action: AutonomousSolanaAuditAction::Coverage {
                        project_root: "/tmp/xero-fixtures".into(),
                        package: None,
                        test_filter: Some("fixture".into()),
                        lcov_path: None,
                        instruction_names: vec!["initialize".into()],
                        timeout_s: Some(5),
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_REPLAY,
                "replay_list",
                AutonomousToolRequest::SolanaReplay(AutonomousSolanaReplayRequest {
                    action: AutonomousSolanaReplayAction::List,
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_INDEXER,
                "indexer_run",
                AutonomousToolRequest::SolanaIndexer(AutonomousSolanaIndexerRequest {
                    action: AutonomousSolanaIndexerAction::Run {
                        cluster: ClusterKind::Devnet,
                        program_ids: vec![program_id.clone()],
                        last_n: Some(5),
                        rpc_url: Some("https://api.devnet.solana.com".into()),
                    },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_SECRETS,
                "secrets_patterns",
                AutonomousToolRequest::SolanaSecrets(AutonomousSolanaSecretsRequest {
                    action: AutonomousSolanaSecretsAction::Patterns,
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_CLUSTER_DRIFT,
                "drift_tracked",
                AutonomousToolRequest::SolanaClusterDrift(AutonomousSolanaDriftRequest {
                    action: AutonomousSolanaDriftAction::Tracked,
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_COST,
                "cost_snapshot",
                AutonomousToolRequest::SolanaCost(AutonomousSolanaCostRequest {
                    action: AutonomousSolanaCostAction::Snapshot { request: None },
                }),
            ),
            (
                AUTONOMOUS_TOOL_SOLANA_DOCS,
                "docs_catalog",
                AutonomousToolRequest::SolanaDocs(AutonomousSolanaDocsRequest {
                    action: AutonomousSolanaDocsAction::Catalog,
                }),
            ),
        ]
    }

    #[test]
    fn solana_runtime_executes_representative_fixture_call_for_every_tool() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_solana_executor(Arc::new(FixtureSolanaExecutor::default()));

        let requests = representative_solana_runtime_requests();
        assert_eq!(requests.len(), 24);

        for (tool_name, expected_action, request) in requests {
            let result = runtime.execute(request).expect(tool_name);
            assert_eq!(result.tool_name, tool_name);
            assert!(result.summary.contains(expected_action));
            let AutonomousToolOutput::Solana(output) = result.output else {
                panic!("{tool_name} should return Solana output");
            };
            assert_eq!(output.action, expected_action);
            let value: JsonValue = serde_json::from_str(&output.value_json).expect("value json");
            assert_eq!(value.get("ok").and_then(JsonValue::as_bool), Some(true));
            assert!(
                value.get("shape").is_some(),
                "{tool_name} should preserve stable shape"
            );
        }
    }

    #[test]
    fn solana_runtime_blocks_mutation_adjacent_calls_without_leaking_payloads() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_solana_executor(Arc::new(FixtureSolanaExecutor {
                deny_mutations: true,
                leak_sensitive_output: false,
            }));
        let program_id = valid_pubkey(3);
        let authority = DeployAuthority::DirectKeypair {
            keypair_path: "/Users/alice/.config/solana/id.json".into(),
        };

        let denied = [
            AutonomousToolRequest::SolanaTx(AutonomousSolanaTxRequest {
                action: AutonomousSolanaTxAction::Send {
                    request: SendRequest {
                        cluster: ClusterKind::Devnet,
                        signed_transaction_base64: "signed-transaction-secret-material".into(),
                        strategy: Default::default(),
                        rpc_url: Some("https://rpc.example/?api-key=live-secret-token".into()),
                        idl_errors: None,
                    },
                },
            }),
            AutonomousToolRequest::SolanaIdl(AutonomousSolanaIdlRequest {
                action: AutonomousSolanaIdlAction::Publish {
                    program_id: program_id.clone(),
                    cluster: ClusterKind::Devnet,
                    idl_path: "/tmp/idl.json".into(),
                    authority_keypair_path: "/Users/alice/.config/solana/id.json".into(),
                    rpc_url: "https://rpc.example/?api-key=live-secret-token".into(),
                    mode: IdlPublishMode::Upgrade,
                },
            }),
            AutonomousToolRequest::SolanaDeploy(AutonomousSolanaDeployRequest {
                program_id: program_id.clone(),
                cluster: ClusterKind::Devnet,
                so_path: "/tmp/program.so".into(),
                authority: authority.clone(),
                idl_path: None,
                is_first_deploy: false,
                post: None,
                rpc_url: Some("https://rpc.example/?api-key=live-secret-token".into()),
                project_root: None,
                block_on_any_secret: false,
            }),
            AutonomousToolRequest::SolanaProgram(AutonomousSolanaProgramRequest {
                action: AutonomousSolanaProgramAction::Rollback {
                    program_id: program_id.clone(),
                    cluster: ClusterKind::Devnet,
                    previous_sha256: "a".repeat(64),
                    authority,
                    program_archive_root: None,
                    post: None,
                    rpc_url: Some("https://rpc.example/?api-key=live-secret-token".into()),
                },
            }),
            AutonomousToolRequest::SolanaVerifiedBuild(AutonomousSolanaVerifiedBuildRequest {
                program_id,
                cluster: ClusterKind::Devnet,
                manifest_path: "/tmp/Cargo.toml".into(),
                github_url: "https://github.com/hyperpush-org/xero".into(),
                commit_hash: None,
                library_name: None,
                skip_remote_submit: false,
            }),
        ];

        for request in denied {
            let err = runtime
                .execute(request)
                .expect_err("mutation call should be denied");
            assert_eq!(err.class, CommandErrorClass::PolicyDenied);
            assert!(err.message.contains("requires explicit approval"));
            let message = err.message.to_ascii_lowercase();
            assert!(!message.contains("signed-transaction-secret-material"));
            assert!(!message.contains("live-secret-token"));
            assert!(!message.contains("/users/alice/.config/solana/id.json"));
        }
    }

    #[test]
    fn solana_runtime_redacts_sensitive_agent_visible_results() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_solana_executor(Arc::new(FixtureSolanaExecutor {
                deny_mutations: false,
                leak_sensitive_output: true,
            }));

        let result = runtime
            .execute(AutonomousToolRequest::SolanaDocs(
                AutonomousSolanaDocsRequest {
                    action: AutonomousSolanaDocsAction::Catalog,
                },
            ))
            .expect("solana docs");
        let AutonomousToolOutput::Solana(output) = result.output else {
            panic!("expected solana output");
        };
        let rendered = output.value_json;
        assert!(rendered.contains("[REDACTED]"));
        assert!(rendered.contains("api-key=redacted"));
        assert!(!rendered.contains("live-secret-token"));
        assert!(!rendered.contains("/Users/alice/.config/solana/id.json"));
        assert!(!rendered.contains("PRIVATE KEY"));
        assert!(!rendered.contains("iVBORw0KGgo"));
        assert!(!rendered.contains("tool_payload"));
    }

    #[test]
    fn solana_catalog_pack_policy_and_request_names_cover_issue_15_inventory() {
        let expected_tools = [
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
        let expected = expected_tools.into_iter().collect::<BTreeSet<_>>();

        let catalog_tools = deferred_tool_catalog(true)
            .into_iter()
            .filter(|entry| entry.group == "solana")
            .map(|entry| entry.tool_name)
            .collect::<BTreeSet<_>>();
        assert_eq!(catalog_tools, expected);

        let pack_tools = domain_tool_pack_tools("solana")
            .expect("solana domain tool pack")
            .into_iter()
            .collect::<BTreeSet<_>>();
        let expected_owned = expected.iter().map(|tool| tool.to_string()).collect();
        assert_eq!(pack_tools, expected_owned);

        let policy = AutonomousAgentToolPolicy::from_definition_snapshot(&json!({
            "toolPolicy": {
                "allowedToolPacks": ["solana"],
                "externalServiceAllowed": true,
                "commandAllowed": true,
                "destructiveWriteAllowed": true
            }
        }))
        .expect("solana pack policy");
        for tool in expected {
            assert!(
                domain_tool_pack_ids_for_tool(tool).contains(&"solana".to_string()),
                "{tool} should be discoverable through the solana domain pack"
            );
            assert!(
                tool_allowed_for_runtime_agent_with_policy(
                    RuntimeAgentIdDto::Engineer,
                    tool,
                    Some(&policy),
                ),
                "{tool} should be callable when the solana pack is explicitly allowed"
            );
        }

        let request_pairs = [
            (
                AutonomousToolRequest::SolanaCluster(AutonomousSolanaClusterRequest {
                    action: AutonomousSolanaClusterAction::List,
                }),
                AUTONOMOUS_TOOL_SOLANA_CLUSTER,
            ),
            (
                AutonomousToolRequest::SolanaLogs(AutonomousSolanaLogsRequest {
                    action: AutonomousSolanaLogsAction::Active,
                }),
                AUTONOMOUS_TOOL_SOLANA_LOGS,
            ),
            (
                AutonomousToolRequest::SolanaDocs(AutonomousSolanaDocsRequest {
                    action: AutonomousSolanaDocsAction::Catalog,
                }),
                AUTONOMOUS_TOOL_SOLANA_DOCS,
            ),
        ];
        for (request, tool_name) in request_pairs {
            assert_eq!(request.tool_name(), tool_name);
        }
    }

    #[test]
    fn computer_use_policy_allows_general_purpose_tools_except_agent_builder_surfaces() {
        let policy = AutonomousAgentToolPolicy::from_policy_label("computer_use");

        for allowed_tool in [
            AUTONOMOUS_TOOL_READ,
            AUTONOMOUS_TOOL_WRITE,
            AUTONOMOUS_TOOL_EDIT,
            AUTONOMOUS_TOOL_PATCH,
            AUTONOMOUS_TOOL_DELETE,
            AUTONOMOUS_TOOL_COMMAND_RUN,
            AUTONOMOUS_TOOL_COMMAND_SESSION,
            AUTONOMOUS_TOOL_PROCESS_MANAGER,
            AUTONOMOUS_TOOL_GIT_STATUS,
            AUTONOMOUS_TOOL_MCP_CALL_TOOL,
            AUTONOMOUS_TOOL_SKILL,
            AUTONOMOUS_TOOL_SUBAGENT,
            AUTONOMOUS_TOOL_TOOL_ACCESS,
            AUTONOMOUS_TOOL_TOOL_SEARCH,
            AUTONOMOUS_TOOL_TODO,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_SEARCH,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_GET,
            AUTONOMOUS_TOOL_PROJECT_CONTEXT_RECORD,
            AUTONOMOUS_TOOL_BROWSER_OBSERVE,
            AUTONOMOUS_TOOL_BROWSER_CONTROL,
            AUTONOMOUS_TOOL_BROWSER,
            AUTONOMOUS_TOOL_EMULATOR,
            AUTONOMOUS_TOOL_MACOS_AUTOMATION,
            AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            AUTONOMOUS_TOOL_DESKTOP_STREAM,
            AUTONOMOUS_TOOL_SYSTEM_DIAGNOSTICS_OBSERVE,
        ] {
            assert!(
                tool_allowed_for_runtime_agent_with_policy(
                    RuntimeAgentIdDto::ComputerUse,
                    allowed_tool,
                    Some(&policy),
                ),
                "computer_use should allow {allowed_tool}"
            );
        }

        for blocked_tool in [
            AUTONOMOUS_TOOL_HARNESS_RUNNER,
            AUTONOMOUS_TOOL_AGENT_DEFINITION,
            AUTONOMOUS_TOOL_WORKFLOW_DEFINITION,
        ] {
            assert!(
                !tool_allowed_for_runtime_agent_with_policy(
                    RuntimeAgentIdDto::ComputerUse,
                    blocked_tool,
                    Some(&policy),
                ),
                "computer_use should block {blocked_tool}"
            );
        }

        assert!(policy.allows_subagent_role(AutonomousSubagentRole::Engineer));
        assert!(policy.allows_subagent_role(AutonomousSubagentRole::Browser));
        assert!(policy.allows_subagent_role(AutonomousSubagentRole::Emulator));
        assert!(!policy.allows_subagent_role(AutonomousSubagentRole::AgentBuilder));
    }

    #[test]
    fn desktop_tool_catalog_metadata_mentions_prompt_visible_runtime_fields() {
        let control = tool_catalog_metadata_for_tool(AUTONOMOUS_TOOL_DESKTOP_CONTROL, true)
            .expect("desktop_control catalog metadata");
        let control_fields = control["schemaFields"]
            .as_array()
            .expect("control schema fields")
            .iter()
            .filter_map(JsonValue::as_str)
            .collect::<Vec<_>>();
        for field in [
            "sourceWidth",
            "sourceHeight",
            "mediaType",
            "imageDataBase64",
            "filePaths",
            "html",
            "rtf",
            "altText",
            "targetLabel",
            "selectionStart",
            "selectionEnd",
        ] {
            assert!(
                control_fields.contains(&field),
                "desktop_control catalog metadata must mention {field}"
            );
        }

        let stream = tool_catalog_metadata_for_tool(AUTONOMOUS_TOOL_DESKTOP_STREAM, true)
            .expect("desktop_stream catalog metadata");
        let stream_fields = stream["schemaFields"]
            .as_array()
            .expect("stream schema fields")
            .iter()
            .filter_map(JsonValue::as_str)
            .collect::<Vec<_>>();
        for field in ["iceServers", "sessionDescription", "iceCandidate"] {
            assert!(
                stream_fields.contains(&field),
                "desktop_stream catalog metadata must mention {field}"
            );
        }
    }

    #[test]
    fn tool_access_list_returns_computer_use_capability_manifest() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path()).expect("runtime");
        let result = runtime
            .tool_access(AutonomousToolAccessRequest {
                action: AutonomousToolAccessAction::List,
                groups: Vec::new(),
                tools: Vec::new(),
                reason: None,
            })
            .expect("tool access list");
        let AutonomousToolOutput::ToolAccess(output) = result.output else {
            panic!("unexpected output");
        };
        let manifest = output
            .capability_manifest
            .expect("Computer Use capability manifest");

        assert_eq!(
            manifest["schema"],
            json!("xero.computer_use_capability_manifest.v1")
        );
        assert_eq!(manifest["runtimeAgentId"], json!("computer_use"));
        let capability_fixtures = manifest["workstationControlPack"]["capabilityReportFixtures"]
            .as_array()
            .expect("platform capability report fixtures");
        assert!(
            capability_fixtures
                .iter()
                .any(|fixture| fixture["platform"] == "windows"
                    && fixture["capabilities"].as_array().is_some_and(|items| items
                        .contains(&json!("native_webrtc_stream"))
                        && items.contains(&json!("dxgi_output_duplication_capture"))
                        && items.contains(&json!("openh264_software_encoding"))
                        && items.contains(&json!("best_effort_cursor_overlay")))),
            "manifest should include a Windows native WebRTC capability report fixture"
        );
        assert!(
            capability_fixtures
                .iter()
                .any(|fixture| fixture["platform"] == "macos"
                    && fixture["capabilities"]
                        .as_array()
                        .is_some_and(|items| items.contains(&json!("native_webrtc_stream")))),
            "manifest should include a macOS capability report fixture"
        );
        let scenario_ids = |fixture: &JsonValue| {
            fixture["verificationScenarios"]
                .as_array()
                .expect("verification scenarios")
                .iter()
                .filter_map(|scenario| scenario["id"].as_str().map(str::to_owned))
                .collect::<Vec<_>>()
        };
        let macos_fixture = capability_fixtures
            .iter()
            .find(|fixture| fixture["platform"] == "macos")
            .expect("macOS capability fixture");
        let macos_scenarios = scenario_ids(macos_fixture);
        for scenario in [
            "macos_screen_recording_denied",
            "macos_screen_recording_granted",
            "macos_accessibility_denied",
            "macos_accessibility_granted",
            "macos_input_monitoring_denied",
            "macos_input_monitoring_granted",
            "macos_multiple_displays",
            "macos_retina_scaling",
            "macos_secure_input",
        ] {
            assert!(
                macos_scenarios.iter().any(|observed| observed == scenario),
                "macOS verification matrix should include {scenario}"
            );
        }
        let windows_fixture = capability_fixtures
            .iter()
            .find(|fixture| fixture["platform"] == "windows")
            .expect("Windows capability fixture");
        let windows_scenarios = scenario_ids(windows_fixture);
        for scenario in [
            "windows_standard_user",
            "windows_administrator",
            "windows_uac_prompt",
            "windows_multiple_dpi_scales",
            "windows_multiple_monitors",
            "windows_rdp_session",
            "windows_store_app",
            "windows_win32_app",
            "windows_electron_app",
            "windows_browser_app",
            "windows_explorer_app",
            "windows_office_style_app",
        ] {
            assert!(
                windows_scenarios
                    .iter()
                    .any(|observed| observed == scenario),
                "Windows verification matrix should include {scenario}"
            );
        }
        let failure_fixture = capability_fixtures
            .iter()
            .find(|fixture| fixture["platform"] == "cross_platform_failure_modes")
            .expect("cross-platform failure mode fixture");
        let failure_scenarios = scenario_ids(failure_fixture);
        for scenario in [
            "sidecar_unavailable",
            "sidecar_operation_unimplemented",
            "screenshot_capture_denied",
            "uia_unavailable",
            "ocr_unavailable",
            "stream_start_failure",
            "local_user_takeover",
        ] {
            assert!(
                failure_scenarios
                    .iter()
                    .any(|observed| observed == scenario),
                "failure-mode verification matrix should include {scenario}"
            );
        }
        let tools = manifest["tools"].as_array().expect("manifest tools");
        let desktop_observe = tools
            .iter()
            .find(|tool| tool["toolName"] == AUTONOMOUS_TOOL_DESKTOP_OBSERVE)
            .expect("desktop_observe manifest row");
        let observe_actions = desktop_observe["actions"]
            .as_array()
            .expect("desktop_observe actions")
            .iter()
            .filter_map(JsonValue::as_str)
            .collect::<Vec<_>>();
        assert!(
            observe_actions.contains(&"clipboard_read_text"),
            "manifest should expose approval-gated clipboard read"
        );
        assert!(
            observe_actions.contains(&"clipboard_read_html"),
            "manifest should expose approval-gated rich clipboard read"
        );
        assert!(
            observe_actions.contains(&"clipboard_read_rtf"),
            "manifest should expose approval-gated RTF clipboard read"
        );
        assert!(
            observe_actions.contains(&"display_arrangement"),
            "manifest should expose prompt-visible display layout diagnostics"
        );
        assert!(
            observe_actions.contains(&"app_inventory"),
            "manifest should expose launch-target app inventory"
        );
        assert!(
            observe_actions.contains(&"notification_snapshot"),
            "manifest should expose approval-gated notification observation"
        );
        assert!(
            observe_actions.contains(&"bridge_affordances"),
            "manifest should expose browser/terminal bridge affordances"
        );
        assert!(
            observe_actions.contains(&"clipboard_read_image")
                && observe_actions.contains(&"clipboard_read_files"),
            "manifest should expose approval-gated clipboard resource reads"
        );
        let desktop_control = tools
            .iter()
            .find(|tool| tool["toolName"] == AUTONOMOUS_TOOL_DESKTOP_CONTROL)
            .expect("desktop_control manifest row");
        let actions = desktop_control["actions"]
            .as_array()
            .expect("desktop_control actions")
            .iter()
            .filter_map(JsonValue::as_str)
            .collect::<Vec<_>>();
        for action in [
            "mouse_down",
            "mouse_drag_move",
            "mouse_up",
            "volume_up",
            "volume_down",
            "volume_mute",
            "media_play_pause",
            "media_next_track",
            "media_prev_track",
            "ax_select",
            "ax_confirm",
            "ax_cancel",
            "ax_increment",
            "ax_decrement",
            "ax_expand",
            "ax_collapse",
            "ax_scroll_to_visible",
            "ax_toggle",
            "clipboard_write_text",
            "clipboard_write_html",
            "clipboard_write_rtf",
            "clipboard_write_image",
            "clipboard_write_files",
            "file_drop",
            "window_maximize",
            "window_minimize",
            "window_restore",
            "window_move_resize",
            "window_close",
            "dock_item_press",
            "status_item_press",
            "file_dialog_set_path",
            "file_dialog_confirm",
        ] {
            assert!(
                actions.contains(&action),
                "manifest should expose desktop_control action {action}"
            );
        }
        assert!(
            desktop_control["availabilityReasons"].is_array(),
            "manifest rows should carry availability diagnostics"
        );
        assert_eq!(
            desktop_control["availabilityDiagnostics"]["schema"],
            json!("xero.computer_use_tool_availability_diagnostics.v1")
        );
        for gate in [
            "policy",
            "providerProjection",
            "rollout",
            "platform",
            "permission",
            "runtime",
            "activation",
        ] {
            assert!(
                desktop_control["availabilityDiagnostics"][gate].is_object(),
                "manifest availability diagnostics should expose {gate} gate"
            );
        }
        assert_eq!(
            desktop_control["availabilityDiagnostics"]["permission"]["missingReasonCode"],
            json!("permission_denied"),
            "desktop tools should point developers to permission-denied diagnostics"
        );
        assert_eq!(
            desktop_control["availabilityDiagnostics"]["providerProjection"]["debugSurface"],
            json!("xero.tool_exposure_diagnostics.v1"),
            "manifest should link provider projection diagnostics to the visible debug surface"
        );
        let powershell = tools
            .iter()
            .find(|tool| tool["toolName"] == AUTONOMOUS_TOOL_POWERSHELL)
            .expect("powershell manifest row");
        assert_eq!(
            powershell["availabilityDiagnostics"]["platform"]["missingReasonCode"],
            json!("platform_unsupported"),
            "unsupported host platform should be an explicit diagnostic, not folded into rollout"
        );
    }

    #[test]
    fn tool_access_request_returns_compact_grant_output() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path()).expect("runtime");
        let result = runtime
            .tool_access(AutonomousToolAccessRequest {
                action: AutonomousToolAccessAction::Request,
                groups: Vec::new(),
                tools: vec![AUTONOMOUS_TOOL_READ.into()],
                reason: Some("Need to inspect a source file.".into()),
            })
            .expect("tool access request");
        let AutonomousToolOutput::ToolAccess(output) = result.output else {
            panic!("unexpected output");
        };

        assert_eq!(output.granted_tools, vec![AUTONOMOUS_TOOL_READ.to_string()]);
        assert!(output.denied_tools.is_empty());
        assert!(output.available_groups.is_empty());
        assert!(output.available_tool_packs.is_empty());
        assert!(output.tool_pack_health.is_empty());
        assert!(output.capability_manifest.is_none());
        assert_eq!(
            output
                .exposure_diagnostics
                .as_ref()
                .and_then(|diagnostics| diagnostics.get("requestReason"))
                .and_then(JsonValue::as_str),
            Some("Need to inspect a source file.")
        );
    }

    #[test]
    fn desktop_tools_are_computer_use_only() {
        for tool in [
            AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            AUTONOMOUS_TOOL_DESKTOP_STREAM,
        ] {
            assert!(tool_allowed_for_runtime_agent(
                RuntimeAgentIdDto::ComputerUse,
                tool
            ));
            assert!(
                !tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Engineer, tool),
                "desktop tool {tool} should stay out of engineering agents"
            );
            assert!(
                !tool_allowed_for_runtime_agent(RuntimeAgentIdDto::Debug, tool),
                "desktop tool {tool} should stay out of debug agents"
            );
        }
    }

    #[test]
    fn desktop_rollout_defaults_to_enabled_for_installed_app() {
        assert!(desktop_tool_available_from_rollout_values(
            AUTONOMOUS_TOOL_DESKTOP_CONTROL,
            None,
            None,
            None,
            "host-1",
            true,
        ));
    }

    #[test]
    fn desktop_rollout_master_flag_enables_all_desktop_tools() {
        assert!(desktop_tool_available_from_rollout_values(
            AUTONOMOUS_TOOL_DESKTOP_STREAM,
            Some("enabled"),
            None,
            None,
            "host-1",
            false,
        ));
    }

    #[test]
    fn desktop_rollout_tool_override_can_disable_single_surface() {
        assert!(!desktop_tool_available_from_rollout_values(
            AUTONOMOUS_TOOL_DESKTOP_STREAM,
            Some("enabled"),
            Some("off"),
            None,
            "host-1",
            true,
        ));
    }

    #[test]
    fn desktop_rollout_percent_is_deterministic_per_host_and_tool() {
        let rollout_id = "host-42";
        let bucket = desktop_rollout_bucket(rollout_id, AUTONOMOUS_TOOL_DESKTOP_OBSERVE);
        let percent = bucket.saturating_add(1).to_string();

        assert_eq!(
            desktop_tool_available_from_rollout_values(
                AUTONOMOUS_TOOL_DESKTOP_OBSERVE,
                None,
                None,
                Some(&percent),
                rollout_id,
                false,
            ),
            bucket < 100,
        );
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
                workflow_structure: None,
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
                workflow_structure: None,
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
    fn durable_subagent_task_round_trip_preserves_child_stages() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path()).expect("runtime");
        let workflow_structure = json!({
            "startPhaseId": "inspect",
            "phases": [{
                "id": "inspect",
                "title": "Inspect",
                "allowedTools": ["read"]
            }]
        });
        let result = runtime
            .subagent(AutonomousSubagentRequest {
                action: AutonomousSubagentAction::Spawn,
                task_id: None,
                role: Some(AutonomousSubagentRole::Researcher),
                prompt: Some("Inspect the child Stage state.".into()),
                model_id: None,
                workflow_structure: Some(workflow_structure.clone()),
                timeout_ms: None,
                max_tool_calls: None,
                max_tokens: None,
                max_cost_micros: None,
                write_set: Vec::new(),
                decision: None,
            })
            .expect("spawn staged child");
        let AutonomousToolOutput::Subagent(output) = result.output else {
            panic!("expected subagent output");
        };
        let record = agent_subagent_task_record_from_task("project-1", "parent-run", &output.task)
            .expect("encode durable task");
        let decoded = autonomous_subagent_task_from_record(record).expect("decode durable task");

        assert_eq!(decoded.workflow_structure, Some(workflow_structure));
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
            .with_runtime_run_controls(engineer_runtime_controls())
            .with_agent_workflow_policy(Some(policy));

        let denied = runtime
            .execute(AutonomousToolRequest::Write(AutonomousWriteRequest {
                path: "notes.txt".into(),
                content: "premature write\n".into(),
                expected_hash: None,
                create_only: false,
                overwrite: None,
                preview: false,
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
                expected_hash: None,
                create_only: false,
                overwrite: None,
                preview: false,
            }))
            .expect("write allowed after workflow gate");
        assert_eq!(
            std::fs::read_to_string(tempdir.path().join("notes.txt")).expect("read written file"),
            "gated write\n"
        );
    }

    #[test]
    fn s22_tool_access_respects_current_workflow_stage() {
        let policy = AutonomousAgentWorkflowPolicy::from_definition_snapshot(&json!({
            "workflowStructure": {
                "startPhaseId": "inspect",
                "phases": [
                    {
                        "id": "inspect",
                        "title": "Inspect",
                        "allowedTools": [
                            AUTONOMOUS_TOOL_READ,
                            AUTONOMOUS_TOOL_TOOL_ACCESS,
                            AUTONOMOUS_TOOL_TODO
                        ],
                        "requiredChecks": [
                            {"kind": "todo_completed", "todoId": "inspect_done"}
                        ]
                    },
                    {
                        "id": "edit",
                        "title": "Edit",
                        "allowedTools": [
                            AUTONOMOUS_TOOL_PATCH,
                            AUTONOMOUS_TOOL_TOOL_ACCESS,
                            AUTONOMOUS_TOOL_TODO
                        ]
                    }
                ]
            }
        }))
        .expect("workflow policy");
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_runtime_run_controls(engineer_runtime_controls())
            .with_agent_workflow_policy(Some(policy));

        let blocked = tool_access_output(runtime.tool_access(AutonomousToolAccessRequest {
            action: AutonomousToolAccessAction::Request,
            groups: vec!["mutation".into()],
            tools: Vec::new(),
            reason: Some("need to edit".into()),
        }));
        assert!(blocked.granted_tools.is_empty());
        assert!(blocked.denied_tools.contains(&"mutation".to_string()));

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

        let granted = tool_access_output(runtime.tool_access(AutonomousToolAccessRequest {
            action: AutonomousToolAccessAction::Request,
            groups: Vec::new(),
            tools: vec![AUTONOMOUS_TOOL_PATCH.into()],
            reason: Some("apply patch".into()),
        }));
        assert_eq!(
            granted.granted_tools,
            vec![AUTONOMOUS_TOOL_PATCH.to_string()]
        );
        assert!(granted.denied_tools.is_empty());
    }

    #[test]
    fn s22_tool_access_grants_web_when_current_workflow_stage_allows_it() {
        let policy = AutonomousAgentWorkflowPolicy::from_definition_snapshot(&json!({
            "workflowStructure": {
                "startPhaseId": "implement",
                "phases": [
                    {
                        "id": "implement",
                        "title": "Implement",
                        "allowedTools": [
                            AUTONOMOUS_TOOL_TOOL_ACCESS,
                            AUTONOMOUS_TOOL_WEB_SEARCH,
                            AUTONOMOUS_TOOL_WEB_FETCH,
                            AUTONOMOUS_TOOL_TODO
                        ]
                    }
                ]
            }
        }))
        .expect("workflow policy");
        let tempdir = tempfile::tempdir().expect("tempdir");
        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_runtime_run_controls(engineer_runtime_controls())
            .with_agent_workflow_policy(Some(policy));

        let granted = tool_access_output(runtime.tool_access(AutonomousToolAccessRequest {
            action: AutonomousToolAccessAction::Request,
            groups: vec!["web_search_only".into(), "web_fetch".into()],
            tools: Vec::new(),
            reason: Some("research documented scaffold command".into()),
        }));

        assert!(granted
            .granted_tools
            .contains(&AUTONOMOUS_TOOL_WEB_SEARCH.to_string()));
        assert!(granted
            .granted_tools
            .contains(&AUTONOMOUS_TOOL_WEB_FETCH.to_string()));
        assert!(granted.denied_tools.is_empty());
    }

    #[test]
    fn s22_workflow_replay_restores_stage_gates_after_resume() {
        let policy = AutonomousAgentWorkflowPolicy::from_definition_snapshot(&json!({
            "workflowStructure": {
                "startPhaseId": "survey",
                "phases": [
                    {
                        "id": "survey",
                        "title": "Survey",
                        "allowedTools": [
                            AUTONOMOUS_TOOL_READ,
                            AUTONOMOUS_TOOL_TOOL_ACCESS,
                            AUTONOMOUS_TOOL_TODO
                        ],
                        "requiredChecks": [
                            {
                                "kind": "tool_succeeded",
                                "toolName": AUTONOMOUS_TOOL_READ,
                                "minCount": 2
                            }
                        ]
                    },
                    {
                        "id": "plan",
                        "title": "Plan",
                        "allowedTools": [
                            AUTONOMOUS_TOOL_TODO,
                            AUTONOMOUS_TOOL_TOOL_ACCESS
                        ],
                        "requiredChecks": [
                            {
                                "kind": "todo_completed",
                                "todoId": "implementation_plan"
                            }
                        ]
                    },
                    {
                        "id": "implement",
                        "title": "Implement",
                        "allowedTools": [
                            AUTONOMOUS_TOOL_WRITE,
                            AUTONOMOUS_TOOL_MKDIR,
                            AUTONOMOUS_TOOL_TOOL_ACCESS,
                            AUTONOMOUS_TOOL_TODO
                        ]
                    }
                ]
            }
        }))
        .expect("workflow policy");
        let tempdir = tempfile::tempdir().expect("tempdir");
        let mut todo_items = BTreeMap::new();
        todo_items.insert(
            "implementation_plan".into(),
            AutonomousTodoItem {
                id: "implementation_plan".into(),
                title: "Implementation plan".into(),
                notes: Some("Selected stack and planned the landing page.".into()),
                status: AutonomousTodoStatus::Completed,
                mode: AutonomousTodoMode::Plan,
                debug_stage: None,
                evidence: None,
                phase_id: Some("plan".into()),
                phase_title: Some("Plan".into()),
                slice_id: Some("P0-S1".into()),
                handoff_note: None,
                updated_at: "2026-06-28T02:05:35Z".into(),
            },
        );
        let mut tool_successes = BTreeMap::new();
        tool_successes.insert(AUTONOMOUS_TOOL_READ.into(), 2);

        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_runtime_run_controls(engineer_runtime_controls())
            .with_agent_workflow_policy(Some(policy))
            .with_agent_workflow_replay(AutonomousAgentWorkflowReplay {
                todo_items,
                tool_successes,
            });

        let granted = tool_access_output(runtime.tool_access(AutonomousToolAccessRequest {
            action: AutonomousToolAccessAction::Request,
            groups: Vec::new(),
            tools: vec![AUTONOMOUS_TOOL_WRITE.into(), AUTONOMOUS_TOOL_MKDIR.into()],
            reason: Some("continue implementation after user input".into()),
        }));

        assert!(granted.denied_tools.is_empty());
        assert!(granted
            .granted_tools
            .contains(&AUTONOMOUS_TOOL_WRITE.to_string()));
        assert!(granted
            .granted_tools
            .contains(&AUTONOMOUS_TOOL_MKDIR.to_string()));
    }

    #[test]
    fn s22_custom_workflow_tool_names_gate_accepts_patch_success() {
        let policy = AutonomousAgentWorkflowPolicy::from_definition_snapshot(&json!({
            "workflowStructure": {
                "startPhaseId": "implement",
                "phases": [
                    {
                        "id": "implement",
                        "title": "Implement",
                        "allowedTools": [AUTONOMOUS_TOOL_PATCH, AUTONOMOUS_TOOL_READ],
                        "requiredChecks": [
                            {
                                "kind": "tool_succeeded",
                                "toolNames": [AUTONOMOUS_TOOL_EDIT, AUTONOMOUS_TOOL_PATCH],
                                "minCount": 1
                            }
                        ]
                    },
                    {
                        "id": "verify",
                        "title": "Verify",
                        "allowedTools": [AUTONOMOUS_TOOL_READ]
                    }
                ]
            }
        }))
        .expect("workflow policy");
        let tempdir = tempfile::tempdir().expect("tempdir");
        std::fs::write(tempdir.path().join("notes.txt"), "before\n").expect("write fixture");
        let runtime = AutonomousToolRuntime::new(tempdir.path())
            .expect("runtime")
            .with_agent_workflow_policy(Some(policy));

        runtime
            .execute(AutonomousToolRequest::Patch(AutonomousPatchRequest {
                path: Some("notes.txt".into()),
                search: Some("before\n".into()),
                replace: Some("after\n".into()),
                replace_all: false,
                expected_hash: None,
                preview: false,
                operations: Vec::new(),
            }))
            .expect("patch satisfies mutation gate");

        let denied = runtime
            .execute(AutonomousToolRequest::Patch(AutonomousPatchRequest {
                path: Some("notes.txt".into()),
                search: Some("after\n".into()),
                replace: Some("again\n".into()),
                replace_all: false,
                expected_hash: None,
                preview: false,
                operations: Vec::new(),
            }))
            .expect_err("workflow advanced to verify after patch");
        assert_eq!(denied.code, "policy_denied");
        assert!(denied.message.contains("required gates"));

        runtime
            .execute(AutonomousToolRequest::Read(AutonomousReadRequest {
                path: "notes.txt".into(),
                system_path: false,
                mode: None,
                start_line: None,
                line_count: None,
                cursor: None,
                around_pattern: None,
                max_bytes_per_file: None,
                byte_offset: None,
                byte_count: None,
                include_line_hashes: false,
            }))
            .expect("read allowed in verify stage");
        assert_eq!(
            std::fs::read_to_string(tempdir.path().join("notes.txt")).expect("read fixture"),
            "after\n"
        );
    }

    fn tool_access_output(
        result: CommandResult<AutonomousToolResult>,
    ) -> AutonomousToolAccessOutput {
        match result.expect("tool access").output {
            AutonomousToolOutput::ToolAccess(output) => output,
            output => panic!("unexpected output: {output:?}"),
        }
    }

    fn engineer_runtime_controls() -> RuntimeRunControlStateDto {
        RuntimeRunControlStateDto {
            active: crate::commands::RuntimeRunActiveControlSnapshotDto {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: Some("engineer".into()),
                agent_definition_version: Some(4),
                provider_profile_id: None,
                model_id: "test-model".into(),
                thinking_effort: None,
                approval_mode: RuntimeRunApprovalModeDto::Suggest,
                plan_mode_required: false,
                auto_compact_enabled: true,
                revision: 1,
                applied_at: "2026-06-06T00:00:00Z".into(),
            },
            pending: None,
        }
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
