pub mod browser;
pub mod emulator;
mod filesystem;
mod git;
mod policy;
mod priority_tools;
mod process;
mod process_manager;
mod repo_scope;
mod skills;
pub mod solana;

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use super::autonomous_web_runtime::{
    AutonomousWebConfig, AutonomousWebFetchContentKind, AutonomousWebFetchOutput,
    AutonomousWebFetchRequest, AutonomousWebRuntime, AutonomousWebSearchOutput,
    AutonomousWebSearchRequest, AUTONOMOUS_TOOL_WEB_FETCH, AUTONOMOUS_TOOL_WEB_SEARCH,
};

use super::autonomous_skill_runtime::{
    load_skill_source_settings_from_path, AutonomousSkillRuntime, AutonomousSkillRuntimeConfig,
    CadenceSkillSourceKind, CadenceSkillSourceState, CadenceSkillToolAccessDecision,
    CadenceSkillToolContextPayload, CadenceSkillToolDiagnostic, CadenceSkillToolInput,
    CadenceSkillToolLifecycleEvent, CadenceSkillToolOperation, CadenceSkillTrustState,
};
use crate::{
    commands::{
        BranchSummaryDto, CommandError, CommandResult, RepositoryDiffScope,
        RepositoryStatusEntryDto, RuntimeRunApprovalModeDto, RuntimeRunControlStateDto,
    },
    runtime::AgentRunCancellationToken,
    state::DesktopState,
};

pub use browser::{
    AutonomousBrowserAction, AutonomousBrowserOutput, AutonomousBrowserRequest, BrowserExecutor,
    UnavailableBrowserExecutor, AUTONOMOUS_TOOL_BROWSER,
};
pub use emulator::{
    AutonomousEmulatorAction, AutonomousEmulatorOutput, AutonomousEmulatorRequest,
    EmulatorExecutor, UnavailableEmulatorExecutor, AUTONOMOUS_TOOL_EMULATOR,
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
pub const AUTONOMOUS_TOOL_MCP: &str = "mcp";
pub const AUTONOMOUS_TOOL_SUBAGENT: &str = "subagent";
pub const AUTONOMOUS_TOOL_TODO: &str = "todo";
pub const AUTONOMOUS_TOOL_NOTEBOOK_EDIT: &str = "notebook_edit";
pub const AUTONOMOUS_TOOL_CODE_INTEL: &str = "code_intel";
pub const AUTONOMOUS_TOOL_LSP: &str = "lsp";
pub const AUTONOMOUS_TOOL_POWERSHELL: &str = "powershell";
pub const AUTONOMOUS_TOOL_TOOL_SEARCH: &str = "tool_search";
pub const AUTONOMOUS_TOOL_SKILL: &str = "skill";

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
const TOOL_ACCESS_WEB_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_WEB_SEARCH,
    AUTONOMOUS_TOOL_WEB_FETCH,
    AUTONOMOUS_TOOL_BROWSER,
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
const TOOL_ACCESS_AGENT_OPS_TOOLS: &[&str] = &[
    AUTONOMOUS_TOOL_SUBAGENT,
    AUTONOMOUS_TOOL_TODO,
    AUTONOMOUS_TOOL_TOOL_SEARCH,
];
const TOOL_ACCESS_MCP_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_MCP];
const TOOL_ACCESS_INTELLIGENCE_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_CODE_INTEL, AUTONOMOUS_TOOL_LSP];
const TOOL_ACCESS_NOTEBOOK_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_NOTEBOOK_EDIT];
const TOOL_ACCESS_POWERSHELL_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_POWERSHELL];
const TOOL_ACCESS_SKILL_TOOLS: &[&str] = &[AUTONOMOUS_TOOL_SKILL];

pub fn tool_access_group_tools(group: &str) -> Option<&'static [&'static str]> {
    match group.trim() {
        "core" => Some(TOOL_ACCESS_CORE_TOOLS),
        "mutation" => Some(TOOL_ACCESS_MUTATION_TOOLS),
        "command" => Some(TOOL_ACCESS_COMMAND_TOOLS),
        "process_manager" => Some(TOOL_ACCESS_PROCESS_MANAGER_TOOLS),
        "web" => Some(TOOL_ACCESS_WEB_TOOLS),
        "emulator" => Some(TOOL_ACCESS_EMULATOR_TOOLS),
        "solana" => Some(TOOL_ACCESS_SOLANA_TOOLS),
        "agent_ops" => Some(TOOL_ACCESS_AGENT_OPS_TOOLS),
        "mcp" => Some(TOOL_ACCESS_MCP_TOOLS),
        "intelligence" => Some(TOOL_ACCESS_INTELLIGENCE_TOOLS),
        "notebook" => Some(TOOL_ACCESS_NOTEBOOK_TOOLS),
        "powershell" => Some(TOOL_ACCESS_POWERSHELL_TOOLS),
        "skills" => Some(TOOL_ACCESS_SKILL_TOOLS),
        _ => None,
    }
}

pub fn tool_access_all_known_tools() -> std::collections::BTreeSet<&'static str> {
    [
        TOOL_ACCESS_CORE_TOOLS,
        TOOL_ACCESS_MUTATION_TOOLS,
        TOOL_ACCESS_COMMAND_TOOLS,
        TOOL_ACCESS_PROCESS_MANAGER_TOOLS,
        TOOL_ACCESS_WEB_TOOLS,
        TOOL_ACCESS_EMULATOR_TOOLS,
        TOOL_ACCESS_SOLANA_TOOLS,
        TOOL_ACCESS_AGENT_OPS_TOOLS,
        TOOL_ACCESS_MCP_TOOLS,
        TOOL_ACCESS_INTELLIGENCE_TOOLS,
        TOOL_ACCESS_NOTEBOOK_TOOLS,
        TOOL_ACCESS_POWERSHELL_TOOLS,
        TOOL_ACCESS_SKILL_TOOLS,
    ]
    .into_iter()
    .flat_map(|tools| tools.iter().copied())
    .collect()
}

pub fn tool_access_group_descriptors() -> Vec<AutonomousToolAccessGroup> {
    [
        ("core", TOOL_ACCESS_CORE_TOOLS),
        ("mutation", TOOL_ACCESS_MUTATION_TOOLS),
        ("command", TOOL_ACCESS_COMMAND_TOOLS),
        ("process_manager", TOOL_ACCESS_PROCESS_MANAGER_TOOLS),
        ("web", TOOL_ACCESS_WEB_TOOLS),
        ("emulator", TOOL_ACCESS_EMULATOR_TOOLS),
        ("solana", TOOL_ACCESS_SOLANA_TOOLS),
        ("agent_ops", TOOL_ACCESS_AGENT_OPS_TOOLS),
        ("mcp", TOOL_ACCESS_MCP_TOOLS),
        ("intelligence", TOOL_ACCESS_INTELLIGENCE_TOOLS),
        ("notebook", TOOL_ACCESS_NOTEBOOK_TOOLS),
        ("powershell", TOOL_ACCESS_POWERSHELL_TOOLS),
        ("skills", TOOL_ACCESS_SKILL_TOOLS),
    ]
    .into_iter()
    .map(|(name, tools)| AutonomousToolAccessGroup {
        name: name.into(),
        tools: tools.iter().map(|tool| (*tool).to_owned()).collect(),
    })
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
    pub(super) browser_executor: Option<Arc<dyn BrowserExecutor>>,
    pub(super) emulator_executor: Option<Arc<dyn EmulatorExecutor>>,
    pub(super) solana_executor: Option<Arc<dyn SolanaExecutor>>,
    pub(super) cancellation_token: Option<AgentRunCancellationToken>,
    pub(super) mcp_registry_path: Option<PathBuf>,
    pub(super) todo_items: Arc<Mutex<BTreeMap<String, AutonomousTodoItem>>>,
    pub(super) subagent_tasks: Arc<Mutex<BTreeMap<String, AutonomousSubagentTask>>>,
    pub(super) subagent_executor: Option<Arc<dyn AutonomousSubagentExecutor>>,
    pub(super) subagent_execution_depth: usize,
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
            .field("mcp_registry_path", &self.mcp_registry_path)
            .field("subagent_execution_depth", &self.subagent_execution_depth)
            .field("skill_tool_enabled", &self.skill_tool.is_some())
            .finish_non_exhaustive()
    }
}

pub trait AutonomousSubagentExecutor: Send + Sync {
    fn execute_subagent(
        &self,
        task: AutonomousSubagentTask,
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
                    "Cadence could not access the imported repository root at {}: {error}",
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
            browser_executor: None,
            emulator_executor: None,
            solana_executor: None,
            cancellation_token: None,
            mcp_registry_path: None,
            todo_items: Arc::new(Mutex::new(BTreeMap::new())),
            subagent_tasks: Arc::new(Mutex::new(BTreeMap::new())),
            subagent_executor: None,
            subagent_execution_depth: 0,
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
        let skill_settings =
            load_skill_source_settings_from_path(&state.skill_source_settings_file(app)?)?;
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
        .with_browser_executor(browser_executor)
        .with_emulator_executor(emulator::tauri_emulator_executor(app.clone()))
        .with_mcp_registry_path(state.mcp_registry_file(app)?)
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
        self.skill_tool = Some(AutonomousSkillToolRuntime::new(
            project_id.into(),
            github_runtime,
            bundled_roots,
            local_roots,
            project_skills_enabled,
            github_enabled,
            plugin_roots,
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
            AutonomousToolRequest::Mcp(request) => self.mcp(request),
            AutonomousToolRequest::Subagent(request) => self.subagent(request),
            AutonomousToolRequest::Todo(request) => self.todo(request),
            AutonomousToolRequest::NotebookEdit(request) => self.notebook_edit(request),
            AutonomousToolRequest::CodeIntel(request) => self.code_intel(request),
            AutonomousToolRequest::Lsp(request) => self.lsp(request),
            AutonomousToolRequest::PowerShell(request) => self.powershell(request),
            AutonomousToolRequest::ToolSearch(request) => self.tool_search(request),
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
                                if self.tool_available_by_runtime(tool) {
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
                    if known_tools.contains(tool.as_str())
                        && self.tool_available_by_runtime(tool.as_str())
                    {
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
            .filter(|group| {
                group.name != "skills"
                    || group
                        .tools
                        .iter()
                        .any(|tool| self.tool_available_by_runtime(tool))
            })
            .collect()
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
    Mcp(AutonomousMcpRequest),
    Subagent(AutonomousSubagentRequest),
    Todo(AutonomousTodoRequest),
    NotebookEdit(AutonomousNotebookEditRequest),
    CodeIntel(AutonomousCodeIntelRequest),
    Lsp(AutonomousLspRequest),
    #[serde(rename = "powershell")]
    PowerShell(AutonomousPowerShellRequest),
    ToolSearch(AutonomousToolSearchRequest),
    Skill(CadenceSkillToolInput),
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
    pub start_line: Option<usize>,
    pub line_count: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchRequest {
    pub query: String,
    pub path: Option<String>,
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
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum AutonomousProcessOwnershipScope {
    CadenceOwned,
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
pub enum AutonomousSubagentType {
    Explore,
    Plan,
    General,
    Verify,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentRequest {
    pub agent_type: AutonomousSubagentType,
    pub prompt: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSubagentTask {
    pub subagent_id: String,
    pub agent_type: AutonomousSubagentType,
    pub prompt: String,
    pub model_id: Option<String>,
    pub status: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_summary: Option<String>,
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
    reason = "Tool outputs are serialized at the command boundary; boxing would add churn without reducing retained payload size."
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
    Mcp(AutonomousMcpOutput),
    Subagent(AutonomousSubagentOutput),
    Todo(AutonomousTodoOutput),
    NotebookEdit(AutonomousNotebookEditOutput),
    CodeIntel(AutonomousCodeIntelOutput),
    Lsp(AutonomousLspOutput),
    ToolSearch(AutonomousToolSearchOutput),
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
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSearchOutput {
    pub query: String,
    pub scope: Option<String>,
    pub matches: Vec<AutonomousSearchMatch>,
    pub scanned_files: usize,
    pub truncated: bool,
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
    pub tools: Vec<String>,
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
    pub process_group_id: Option<i64>,
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
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousToolSearchOutput {
    pub query: String,
    pub matches: Vec<AutonomousToolSearchMatch>,
    pub truncated: bool,
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

#[derive(Clone)]
pub(super) struct AutonomousSkillToolRuntime {
    project_id: String,
    github_runtime: AutonomousSkillRuntime,
    bundled_roots: Vec<AutonomousBundledSkillRoot>,
    local_roots: Vec<AutonomousLocalSkillRoot>,
    project_skills_enabled: bool,
    github_enabled: bool,
    plugin_roots: Vec<AutonomousPluginRoot>,
    discovery_cache: Arc<Mutex<BTreeMap<String, skills::CachedSkillToolCandidate>>>,
}

impl AutonomousSkillToolRuntime {
    fn new(
        project_id: String,
        github_runtime: AutonomousSkillRuntime,
        bundled_roots: Vec<AutonomousBundledSkillRoot>,
        local_roots: Vec<AutonomousLocalSkillRoot>,
        project_skills_enabled: bool,
        github_enabled: bool,
        plugin_roots: Vec<AutonomousPluginRoot>,
    ) -> Self {
        Self {
            project_id,
            github_runtime,
            bundled_roots,
            local_roots,
            project_skills_enabled,
            github_enabled,
            plugin_roots,
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
                bundle_id: "cadence".into(),
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
    pub source_kind: CadenceSkillSourceKind,
    pub state: CadenceSkillSourceState,
    pub trust: CadenceSkillTrustState,
    pub enabled: bool,
    pub installed: bool,
    pub user_invocable: Option<bool>,
    pub version_hash: Option<String>,
    pub cache_key: Option<String>,
    pub access: CadenceSkillToolAccessDecision,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutonomousSkillToolOutput {
    pub operation: CadenceSkillToolOperation,
    pub status: AutonomousSkillToolStatus,
    pub message: String,
    pub candidates: Vec<AutonomousSkillToolCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected: Option<AutonomousSkillToolCandidate>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<CadenceSkillToolContextPayload>,
    #[serde(default)]
    pub lifecycle_events: Vec<CadenceSkillToolLifecycleEvent>,
    #[serde(default)]
    pub diagnostics: Vec<CadenceSkillToolDiagnostic>,
    pub truncated: bool,
}
