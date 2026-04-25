pub mod browser;
pub mod emulator;
mod filesystem;
mod git;
mod policy;
mod process;
mod repo_scope;
pub mod solana;

use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, Runtime};

use super::autonomous_web_runtime::{
    AutonomousWebConfig, AutonomousWebFetchContentKind, AutonomousWebFetchOutput,
    AutonomousWebFetchRequest, AutonomousWebRuntime, AutonomousWebSearchOutput,
    AutonomousWebSearchRequest, AUTONOMOUS_TOOL_WEB_FETCH, AUTONOMOUS_TOOL_WEB_SEARCH,
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

pub fn tool_access_group_tools(group: &str) -> Option<&'static [&'static str]> {
    match group.trim() {
        "core" => Some(TOOL_ACCESS_CORE_TOOLS),
        "mutation" => Some(TOOL_ACCESS_MUTATION_TOOLS),
        "command" => Some(TOOL_ACCESS_COMMAND_TOOLS),
        "web" => Some(TOOL_ACCESS_WEB_TOOLS),
        "emulator" => Some(TOOL_ACCESS_EMULATOR_TOOLS),
        "solana" => Some(TOOL_ACCESS_SOLANA_TOOLS),
        _ => None,
    }
}

pub fn tool_access_all_known_tools() -> std::collections::BTreeSet<&'static str> {
    [
        TOOL_ACCESS_CORE_TOOLS,
        TOOL_ACCESS_MUTATION_TOOLS,
        TOOL_ACCESS_COMMAND_TOOLS,
        TOOL_ACCESS_WEB_TOOLS,
        TOOL_ACCESS_EMULATOR_TOOLS,
        TOOL_ACCESS_SOLANA_TOOLS,
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
        ("web", TOOL_ACCESS_WEB_TOOLS),
        ("emulator", TOOL_ACCESS_EMULATOR_TOOLS),
        ("solana", TOOL_ACCESS_SOLANA_TOOLS),
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

#[derive(Debug, Clone)]
pub struct AutonomousToolRuntime {
    pub(super) repo_root: PathBuf,
    pub(super) limits: AutonomousToolRuntimeLimits,
    pub(super) web_runtime: AutonomousWebRuntime,
    pub(super) command_controls: Option<RuntimeRunControlStateDto>,
    pub(super) browser_executor: Option<Arc<dyn BrowserExecutor>>,
    pub(super) emulator_executor: Option<Arc<dyn EmulatorExecutor>>,
    pub(super) solana_executor: Option<Arc<dyn SolanaExecutor>>,
    pub(super) cancellation_token: Option<AgentRunCancellationToken>,
    process_sessions: Arc<process::ProcessSessionRegistry>,
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
            process_sessions: Arc::new(process::ProcessSessionRegistry::default()),
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
        let repo_root = resolve_imported_repo_root(app, state, project_id)?;
        let browser_executor = browser::tauri_browser_executor(app.clone(), state.clone());
        let runtime = Self::with_limits_and_web_config(
            repo_root,
            AutonomousToolRuntimeLimits::default(),
            state.autonomous_web_config(),
        )?
        .with_browser_executor(browser_executor)
        .with_emulator_executor(emulator::tauri_emulator_executor(app.clone()));

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
                available_groups: tool_access_group_descriptors(),
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
                            requested.extend(tools.iter().map(|tool| (*tool).to_owned()))
                        }
                        None => {
                            denied.insert(group);
                        }
                    }
                }

                let known_tools = tool_access_all_known_tools();
                for tool in request.tools {
                    if known_tools.contains(tool.as_str()) {
                        requested.insert(tool);
                    } else {
                        denied.insert(tool);
                    }
                }

                AutonomousToolAccessOutput {
                    action: "request".into(),
                    granted_tools: requested.into_iter().collect(),
                    denied_tools: denied.into_iter().collect(),
                    available_groups: tool_access_group_descriptors(),
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
