//! Terminal & project-runner commands. Powers the right-side Terminal
//! sidebar (xterm.js front-end) and the titlebar Play button.
//!
//! Each terminal tab is a full PTY backed by `portable-pty`. The Play button
//! is just a convenience that opens a new terminal tab and writes a project
//! start target's command to its stdin. A project can define multiple named
//! targets (e.g. frontend, backend, worker) for monorepo or multi-process
//! setups.
//!
//! Persistence: `start_targets TEXT` (JSON array) lives on the `projects`
//! table in both the global DB (project list) and the per-project DB
//! (project detail). Reads come from the per-project DB via
//! `read_project_row`; writes target both so the two views stay in sync.
//!
//! Process model: terminals are keyed by a server-issued `terminalId`. A
//! reader thread per PTY drains the master and emits `terminal:data` events.
//! Calling `terminal_close` (or the user closing the tab) kills the child
//! and removes the entry from the registry. Status changes go out as
//! `terminal:status` events.

use std::collections::{HashMap, VecDeque};
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, LazyLock, Mutex,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use rand::RngCore;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::auth::now_timestamp;
use crate::commands::{
    default_runtime_agent_id,
    provider_credentials::load_provider_credentials_view,
    runtime_support::{emit_project_updated, resolve_owned_agent_provider_config},
    validate_non_empty, CommandError, CommandResult, ProjectSummaryDto, ProjectUpdateReason,
    ProviderModelThinkingEffortDto, RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto,
    RuntimeRunApprovalModeDto, RuntimeRunControlInputDto, RuntimeRunControlStateDto,
    StartTargetDto,
};
use crate::db::{database_path_for_repo, project_app_data_dir_for_repo, project_store};
use crate::global_db::open_global_database;
use crate::provider_credentials::ProviderCredentialsView;
use crate::runtime::autonomous_tool_runtime::resolve_imported_repo_root;
use crate::runtime::{
    create_provider_adapter, ProviderMessage, ProviderStreamEvent, ProviderTurnOutcome,
    ProviderTurnRequest,
};
use crate::state::DesktopState;

pub const TERMINAL_DATA_EVENT: &str = "terminal:data";
pub const TERMINAL_EXIT_EVENT: &str = "terminal:exit";
pub const TERMINAL_TITLE_EVENT: &str = "terminal:title";

const DETECTION_FILE_CAP_BYTES: usize = 4 * 1024;
const TERMINAL_TITLE_POLL_INTERVAL: Duration = Duration::from_millis(750);
const MAX_TERMINAL_BUFFER_BYTES: usize = 256 * 1024;
const DEFAULT_TERMINAL_READ_BYTES: usize = 64 * 1024;
const MAX_TERMINAL_READ_BYTES: usize = 512 * 1024;
const TERMINAL_SUGGESTION_HISTORY_LIMIT: usize = 400;
const TERMINAL_SUGGESTION_MAX_COMMAND_CHARS: usize = 1_000;
const TERMINAL_SUGGESTION_MAX_BUFFER_CHARS: usize = 4_096;
const TERMINAL_SUGGESTION_MAX_CANDIDATES: usize = 8;

const SUGGEST_SYSTEM_PROMPT: &str = "You suggest the shell commands a developer would run to start this project locally. Return a JSON array of {\"name\": \"...\", \"command\": \"...\", \"browserSupported\": true/false} objects and nothing else. No markdown fences, no prose, no explanation.\n\nSet `browserSupported` to true only for commands that start a user-facing web app or dev server that should open in a browser, such as Vite, Next.js, Remix, Astro, SvelteKit, Nuxt, Storybook, Rails/Phoenix/Django/Laravel web servers, or a package named web/client/app/site/docs. Set it to false for backend APIs, workers, CLIs, database services, Tauri/native/mobile dev commands, test runners, codegen, queues, and generic orchestrators unless the command itself clearly launches a browser-served web UI.\n\nIMPORTANT — root orchestrator detection: if the root `package.json` (or `Makefile`/`Procfile`/`mprocs.yaml`/`turbo.json` task) defines a script that fans out to multiple services in one command (via `concurrently`, `npm-run-all`, `turbo run dev`, `nx run-many`, `pnpm -r run`, `make -j`, `mprocs`, `overmind`, `foreman`, `honcho`, etc.), include it as the FIRST target named `all` (or `dev` if that matches the script name). This is the single-command \"run everything\" entry the user reaches for most often.\n\nIn addition to (not instead of) the orchestrator, for monorepos (pnpm/yarn/npm workspaces, Turborepo, Nx, Lerna, Rush, Cargo workspaces, Go workspaces) propose one target per runnable service or app so users can launch them individually. Name each per-service target after the package (e.g. `web`, `api`, `worker`) and inline a `cd <relative-path> && <cmd>` so each command runs from the project root.\n\nFor single-app projects, return one target named `start`.\n\nEach `name` must be short, lowercase, unique, and filename-safe. Each `command` must be a single line of shell.";

// ---------------------------------------------------------------------------
// DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StartTargetInputDto {
    #[serde(default)]
    pub id: Option<String>,
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub browser_supported: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateProjectStartTargetsRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub targets: Vec<StartTargetInputDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SuggestProjectStartTargetsRequestDto {
    pub project_id: String,
    /// Optional provider id paired with `modelId`. Used when the caller only
    /// has a composer selection key (`providerId:modelId`) and not an explicit
    /// provider profile id.
    #[serde(default)]
    pub provider_id: Option<String>,
    /// Optional. If empty/missing the active provider profile's default model
    /// is used. This lets the AI suggest button work even before the agent
    /// pane has been opened in this session, as long as a provider profile
    /// exists.
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub provider_profile_id: Option<String>,
    #[serde(default)]
    pub runtime_agent_id: Option<RuntimeAgentIdDto>,
    #[serde(default)]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedStartTargetDto {
    pub name: String,
    pub command: String,
    pub browser_supported: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedStartTargetsDto {
    pub targets: Vec<SuggestedStartTargetDto>,
    pub provider_id: String,
    pub model_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenTerminalRequestDto {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub client_terminal_id: Option<String>,
    #[serde(default)]
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
    #[serde(default)]
    pub suppress_transcript_until_input: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalTranscriptRequestDto {
    pub project_id: String,
    pub client_terminal_id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTranscriptResponseDto {
    pub project_id: String,
    pub client_terminal_id: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalIdRequestDto {
    pub terminal_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalWriteRequestDto {
    pub terminal_id: String,
    pub data: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalResizeRequestDto {
    pub terminal_id: String,
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalReadRequestDto {
    pub terminal_id: String,
    #[serde(default)]
    pub after_sequence: Option<u64>,
    #[serde(default)]
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenTerminalResponseDto {
    pub terminal_id: String,
    pub shell: String,
    pub cwd: String,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutputChunkDto {
    pub sequence: u64,
    pub data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalReadResponseDto {
    pub terminal_id: String,
    pub running: bool,
    pub next_sequence: u64,
    pub chunks: Vec<TerminalOutputChunkDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSummaryDto {
    pub terminal_id: String,
    pub shell: String,
    pub cwd: String,
    pub started_at: String,
    pub title: String,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalListResponseDto {
    pub terminals: Vec<TerminalSummaryDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalSuggestRequestDto {
    pub project_id: String,
    #[serde(default)]
    pub terminal_id: Option<String>,
    pub buffer: String,
    pub cursor: usize,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub shell: Option<String>,
    #[serde(default)]
    pub recent_block_context: Option<String>,
    pub request_id: u64,
    #[serde(default)]
    pub enable_ai: bool,
    #[serde(default)]
    pub provider_id: Option<String>,
    #[serde(default)]
    pub model_id: Option<String>,
    #[serde(default)]
    pub provider_profile_id: Option<String>,
    #[serde(default)]
    pub runtime_agent_id: Option<RuntimeAgentIdDto>,
    #[serde(default)]
    pub thinking_effort: Option<ProviderModelThinkingEffortDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalRecordCommandRequestDto {
    pub project_id: String,
    pub command: String,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub shell: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalIgnoreSuggestionRequestDto {
    pub project_id: String,
    pub display: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSuggestionReplacementRangeDto {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSuggestionCandidateDto {
    pub replacement: String,
    pub display: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub source: String,
    pub confidence: f32,
    pub replacement_range: TerminalSuggestionReplacementRangeDto,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSuggestResponseDto {
    pub request_id: u64,
    pub candidates: Vec<TerminalSuggestionCandidateDto>,
    pub deterministic_exhausted: bool,
    pub ai_attempted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TerminalHistoryEntry {
    command: String,
    cwd: Option<String>,
    shell: Option<String>,
    used_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalDataEventPayload {
    terminal_id: String,
    data: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalExitEventPayload {
    terminal_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    exit_code: Option<i32>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TerminalTitleEventPayload {
    terminal_id: String,
    title: String,
}

#[derive(Clone)]
struct TerminalEventSink {
    data: Arc<dyn Fn(TerminalDataEventPayload) + Send + Sync>,
    exit: Arc<dyn Fn(TerminalExitEventPayload) + Send + Sync>,
    title: Arc<dyn Fn(TerminalTitleEventPayload) + Send + Sync>,
}

impl TerminalEventSink {
    fn none() -> Self {
        Self {
            data: Arc::new(|_| {}),
            exit: Arc::new(|_| {}),
            title: Arc::new(|_| {}),
        }
    }

    fn tauri<R: Runtime + 'static>(app: AppHandle<R>) -> Self {
        let data_app = app.clone();
        let exit_app = app.clone();
        Self {
            data: Arc::new(move |payload| {
                let _ = data_app.emit(TERMINAL_DATA_EVENT, payload);
            }),
            exit: Arc::new(move |payload| {
                let _ = exit_app.emit(TERMINAL_EXIT_EVENT, payload);
            }),
            title: Arc::new(move |payload| {
                let _ = app.emit(TERMINAL_TITLE_EVENT, payload);
            }),
        }
    }

    fn emit_data(&self, payload: TerminalDataEventPayload) {
        (self.data)(payload);
    }

    fn emit_exit(&self, payload: TerminalExitEventPayload) {
        (self.exit)(payload);
    }

    fn emit_title(&self, payload: TerminalTitleEventPayload) {
        (self.title)(payload);
    }
}

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

struct TerminalHandle {
    alive: AtomicBool,
    terminal_id: String,
    shell: String,
    cwd: String,
    started_at: String,
    title: Mutex<Option<String>>,
    next_sequence: AtomicU64,
    output: Mutex<TerminalOutputBuffer>,
    transcript_target: Option<TerminalTranscriptTarget>,
    transcript_enabled: AtomicBool,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    process: TerminalProcessHandle,
}

struct TerminalProcessHandle {
    child_killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

type TerminalRegistry = HashMap<String, Arc<TerminalHandle>>;

static TERMINALS: LazyLock<Mutex<TerminalRegistry>> = LazyLock::new(|| Mutex::new(HashMap::new()));

#[derive(Clone)]
struct TerminalTranscriptTarget {
    path: PathBuf,
}

struct TerminalOutputBuffer {
    chunks: VecDeque<TerminalOutputChunkDto>,
    retained_bytes: usize,
}

impl TerminalOutputBuffer {
    fn new() -> Self {
        Self {
            chunks: VecDeque::new(),
            retained_bytes: 0,
        }
    }

    fn push(&mut self, sequence: u64, data: String) {
        self.retained_bytes = self.retained_bytes.saturating_add(data.len());
        self.chunks
            .push_back(TerminalOutputChunkDto { sequence, data });
        while self.retained_bytes > MAX_TERMINAL_BUFFER_BYTES {
            let Some(removed) = self.chunks.pop_front() else {
                self.retained_bytes = 0;
                break;
            };
            self.retained_bytes = self.retained_bytes.saturating_sub(removed.data.len());
        }
    }

    fn read_after(
        &self,
        after_sequence: Option<u64>,
        max_bytes: usize,
    ) -> Vec<TerminalOutputChunkDto> {
        let mut chunks = Vec::new();
        let mut bytes = 0usize;
        for chunk in self.chunks.iter().filter(|chunk| {
            after_sequence
                .map(|after| chunk.sequence > after)
                .unwrap_or(true)
        }) {
            if !chunks.is_empty() && bytes.saturating_add(chunk.data.len()) > max_bytes {
                break;
            }
            bytes = bytes.saturating_add(chunk.data.len());
            chunks.push(chunk.clone());
            if bytes >= max_bytes {
                break;
            }
        }
        chunks
    }
}

fn lock_terminals() -> Result<std::sync::MutexGuard<'static, TerminalRegistry>, CommandError> {
    TERMINALS.lock().map_err(|_| {
        CommandError::system_fault(
            "terminal_registry_poisoned",
            "Xero could not access the terminal registry.",
        )
    })
}

fn spawn_terminal_title_watcher(
    sink: TerminalEventSink,
    terminal_id: String,
    handle: Arc<TerminalHandle>,
    shell: String,
    shell_pid: Option<u32>,
) {
    thread::spawn(move || {
        let shell_title = process_label_from_path(&shell).unwrap_or_else(|| "terminal".to_owned());
        let mut last_title: Option<String> = None;

        while handle.alive.load(Ordering::Relaxed) {
            let title = current_terminal_title(&handle, shell_pid, &shell_title)
                .and_then(|value| normalize_terminal_title(&value));

            if let Some(title) = title {
                if last_title.as_deref() != Some(title.as_str()) {
                    if let Ok(mut current_title) = handle.title.lock() {
                        *current_title = Some(title.clone());
                    }
                    sink.emit_title(TerminalTitleEventPayload {
                        terminal_id: terminal_id.clone(),
                        title: title.clone(),
                    });
                    last_title = Some(title);
                }
            }

            thread::sleep(TERMINAL_TITLE_POLL_INTERVAL);
        }
    });
}

#[cfg(unix)]
fn current_terminal_title(
    handle: &TerminalHandle,
    shell_pid: Option<u32>,
    shell_title: &str,
) -> Option<String> {
    let foreground_pgid = {
        let master = handle.master.lock().ok()?;
        master.process_group_leader()?
    };
    if shell_pid.is_some_and(|pid| foreground_pgid == pid as libc::pid_t) {
        return Some(shell_title.to_owned());
    }
    process_label_for_group(foreground_pgid)
}

#[cfg(not(unix))]
fn current_terminal_title(
    _handle: &TerminalHandle,
    _shell_pid: Option<u32>,
    shell_title: &str,
) -> Option<String> {
    Some(shell_title.to_owned())
}

#[cfg(unix)]
fn process_label_for_group(pgid: libc::pid_t) -> Option<String> {
    process_args_for_pid(pgid)
        .and_then(|args| process_label_from_args(&args))
        .or_else(|| {
            process_group_args(pgid)
                .into_iter()
                .find_map(|args| process_label_from_args(&args))
        })
}

#[cfg(unix)]
fn process_args_for_pid(pid: libc::pid_t) -> Option<String> {
    let pid = pid.to_string();
    let output = Command::new("ps")
        .args(["-p", pid.as_str(), "-o", "args="])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let args = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if args.is_empty() {
        None
    } else {
        Some(args)
    }
}

#[cfg(unix)]
fn process_group_args(pgid: libc::pid_t) -> Vec<String> {
    let output = match Command::new("ps")
        .args(["-ax", "-o", "pid=", "-o", "pgid=", "-o", "args="])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(parse_ps_pid_pgid_args)
        .filter(|(_, line_pgid, _)| *line_pgid == pgid)
        .map(|(_, _, args)| args.to_owned())
        .collect()
}

#[cfg(unix)]
fn parse_ps_pid_pgid_args(line: &str) -> Option<(libc::pid_t, libc::pid_t, &str)> {
    let trimmed = line.trim_start();
    let pid_end = trimmed.find(char::is_whitespace)?;
    let pid = trimmed[..pid_end].trim().parse().ok()?;
    let after_pid = trimmed[pid_end..].trim_start();
    let pgid_end = after_pid.find(char::is_whitespace)?;
    let pgid = after_pid[..pgid_end].trim().parse().ok()?;
    let args = after_pid[pgid_end..].trim_start();
    if args.is_empty() {
        return None;
    }
    Some((pid, pgid, args))
}

#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn process_label_from_args(args: &str) -> Option<String> {
    let tokens = split_process_args(args);
    if tokens.is_empty() {
        return None;
    }
    if let Some(label) = tokens.iter().find_map(|token| known_cli_label(token)) {
        return Some(label.to_owned());
    }

    let command_index = first_command_token_index(&tokens)?;
    let command = tokens.get(command_index)?;
    let command_label = process_label_from_path(command)?;

    if is_script_runtime(&command_label) {
        if let Some(label) = tokens
            .iter()
            .skip(command_index + 1)
            .find_map(|token| script_label_from_token(token))
        {
            return Some(label);
        }
    }

    if let Some(label) = package_manager_script_label(&command_label, &tokens[command_index + 1..])
    {
        return Some(label);
    }

    Some(command_label)
}

#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn split_process_args(args: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in args.chars() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(active_quote) = quote {
            if ch == active_quote {
                quote = None;
            } else {
                current.push(ch);
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
            continue;
        }
        if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
            continue;
        }
        current.push(ch);
    }

    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn first_command_token_index(tokens: &[String]) -> Option<usize> {
    let mut index = 0;
    while index < tokens.len() {
        let label = process_label_from_path(&tokens[index])?;
        if !matches!(label.as_str(), "env" | "command" | "nohup") {
            return Some(index);
        }
        index += 1;
        while index < tokens.len()
            && (tokens[index].contains('=') || tokens[index].starts_with('-'))
        {
            index += 1;
        }
    }
    None
}

#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn known_cli_label(token: &str) -> Option<&'static str> {
    let lower = token.to_ascii_lowercase();
    let components = lower.split(['/', '\\', ':', '@']);
    for component in components {
        let component = component.trim_matches(|ch: char| {
            matches!(ch, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}')
        });
        for (needle, label) in [
            ("claude", "claude"),
            ("codex", "codex"),
            ("opencode", "opencode"),
            ("aider", "aider"),
            ("goose", "goose"),
            ("gemini", "gemini"),
            ("cursor-agent", "cursor"),
        ] {
            if component == needle
                || component.starts_with(&format!("{needle}-"))
                || component.ends_with(&format!("-{needle}"))
            {
                return Some(label);
            }
        }
    }
    None
}

#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn is_script_runtime(label: &str) -> bool {
    let lower = label.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "node" | "nodejs" | "bun" | "deno" | "python" | "python3" | "ruby" | "perl"
    ) || lower.starts_with("python3.")
}

#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn script_label_from_token(token: &str) -> Option<String> {
    if token.starts_with('-') || token.contains('=') {
        return None;
    }
    let label = process_label_from_path(token)?;
    if is_script_runtime(&label) {
        return None;
    }
    Some(label)
}

#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn package_manager_script_label(command: &str, args: &[String]) -> Option<String> {
    if !matches!(command, "npm" | "pnpm" | "yarn") {
        return None;
    }
    let script = args
        .iter()
        .skip_while(|arg| arg.as_str() == "run" || arg.starts_with('-'))
        .find(|arg| !arg.trim().is_empty())?;
    let script = normalize_terminal_title(script)?;
    Some(format!("{command} {script}"))
}

fn process_label_from_path(value: &str) -> Option<String> {
    let base = value
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(value)
        .trim_start_matches('-');
    let without_extension = strip_known_command_extension(base);
    normalize_terminal_title(without_extension)
}

fn strip_known_command_extension(value: &str) -> &str {
    for extension in [".js", ".mjs", ".cjs", ".ts", ".py", ".rb"] {
        if let Some(stripped) = value.strip_suffix(extension) {
            return stripped;
        }
    }
    value
}

fn normalize_terminal_title(value: &str) -> Option<String> {
    let compact = value.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = compact.trim_matches(|ch: char| {
        ch.is_control() || matches!(ch, '"' | '\'' | '`' | '(' | ')' | '[' | ']' | '{' | '}')
    });
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(48).collect())
}

fn resolve_requested_provider_profile_id<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopState,
    provider_id: Option<&str>,
) -> CommandResult<Option<String>> {
    let Some(provider_id) = provider_id
        .map(str::trim)
        .filter(|provider_id| !provider_id.is_empty())
    else {
        return Ok(None);
    };

    let provider_profiles = load_provider_credentials_view(app, state)?;
    provider_profile_id_for_provider(&provider_profiles, provider_id)
}

fn provider_profile_id_for_provider(
    provider_profiles: &ProviderCredentialsView,
    provider_id: &str,
) -> CommandResult<Option<String>> {
    let provider_id = provider_id.trim();
    if provider_id.is_empty() {
        return Ok(None);
    }

    provider_profiles
        .profiles()
        .iter()
        .find(|profile| profile.provider_id == provider_id)
        .map(|profile| Some(profile.profile_id.clone()))
        .ok_or_else(|| {
            CommandError::user_fixable(
                "provider_not_found",
                format!(
                    "Xero could not resolve the project-runner suggestion provider because provider `{provider_id}` is missing.",
                ),
            )
        })
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn update_project_start_targets<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: UpdateProjectStartTargetsRequestDto,
) -> CommandResult<ProjectSummaryDto> {
    let project_id = require_non_empty(&request.project_id, "projectId")?.to_owned();
    let targets = normalize_start_target_inputs(request.targets)?;
    let encoded = serde_json::to_string(&targets).map_err(|error| {
        CommandError::system_fault(
            "project_start_targets_encode_failed",
            format!("Xero could not encode start targets: {error}"),
        )
    })?;

    let repo_root = resolve_imported_repo_root(&app, state.inner(), &project_id)?;
    let global_db_path = state.global_db_path(&app)?;

    write_global_db_start_targets(&global_db_path, &project_id, &encoded)?;
    write_project_db_start_targets(&repo_root, &project_id, &encoded)?;

    // Broadcast the fresh snapshot so the active project view in the
    // frontend picks up the new targets without requiring a project switch.
    emit_project_updated(
        &app,
        &repo_root,
        &project_id,
        ProjectUpdateReason::MetadataChanged,
    )?;

    project_store::load_project_summary(&repo_root, &project_id)
}

#[tauri::command]
pub async fn suggest_project_start_targets<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SuggestProjectStartTargetsRequestDto,
) -> CommandResult<SuggestedStartTargetsDto> {
    require_non_empty(&request.project_id, "projectId")?;
    // Hand the blocking provider call off to the worker pool. Synchronous
    // Tauri commands run on the main thread, which freezes the UI for the
    // duration of the HTTP round-trip — exactly the beach-ball the user saw.
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        suggest_project_start_targets_blocking(app, state, request)
    })
    .await
    .map_err(|error| {
        CommandError::system_fault(
            "project_suggest_targets_task_failed",
            format!("Xero could not finish background start-targets suggestion: {error}"),
        )
    })?
}

fn suggest_project_start_targets_blocking<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: SuggestProjectStartTargetsRequestDto,
) -> CommandResult<SuggestedStartTargetsDto> {
    let project_id = request.project_id.trim().to_owned();
    let repo_root = resolve_imported_repo_root(&app, &state, &project_id)?;
    let runtime_agent_id = request
        .runtime_agent_id
        .unwrap_or_else(default_runtime_agent_id);
    let provider_profile_id = if let Some(provider_profile_id) = request
        .provider_profile_id
        .clone()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        Some(provider_profile_id)
    } else {
        resolve_requested_provider_profile_id(&app, &state, request.provider_id.as_deref())?
    };

    let model_id = request
        .model_id
        .as_ref()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let controls = RuntimeRunControlInputDto {
        runtime_agent_id,
        agent_definition_id: None,
        agent_definition_version: None,
        provider_profile_id,
        model_id: model_id.clone(),
        thinking_effort: request.thinking_effort.clone(),
        approval_mode: RuntimeRunApprovalModeDto::Yolo,
        plan_mode_required: false,
        auto_compact_enabled: false,
    };

    let provider_config = resolve_owned_agent_provider_config(&app, &state, Some(&controls))?;
    let provider = create_provider_adapter(provider_config)?;
    let provider_id = provider.provider_id().to_owned();
    let provider_model_id = provider.model_id().to_owned();

    let user_prompt = build_suggest_prompt(&repo_root);
    let turn = ProviderTurnRequest {
        system_prompt: SUGGEST_SYSTEM_PROMPT.into(),
        messages: vec![ProviderMessage::User {
            content: user_prompt,
            attachments: Vec::new(),
        }],
        tools: Vec::new(),
        turn_index: 0,
        controls: RuntimeRunControlStateDto {
            active: RuntimeRunActiveControlSnapshotDto {
                runtime_agent_id,
                agent_definition_id: None,
                agent_definition_version: None,
                provider_profile_id: controls.provider_profile_id.clone(),
                model_id: provider_model_id.clone(),
                thinking_effort: controls.thinking_effort.clone(),
                approval_mode: RuntimeRunApprovalModeDto::Yolo,
                plan_mode_required: false,
                auto_compact_enabled: false,
                revision: 1,
                applied_at: now_timestamp(),
            },
            pending: None,
        },
    };

    let mut emit = |_event: ProviderStreamEvent| Ok(());
    let message = match provider.stream_turn(&turn, &mut emit)? {
        ProviderTurnOutcome::Complete { message, .. } => message,
        ProviderTurnOutcome::ToolCalls { .. } => {
            return Err(CommandError::user_fixable(
                "project_suggest_targets_provider_requested_tools",
                "Xero asked the selected model for start targets, but the model requested tools instead.",
            ));
        }
    };

    let targets = sanitize_suggested_targets(&message)?;
    Ok(SuggestedStartTargetsDto {
        targets,
        provider_id,
        model_id: provider_model_id,
    })
}

#[tauri::command]
pub async fn terminal_open<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: OpenTerminalRequestDto,
) -> CommandResult<OpenTerminalResponseDto> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || terminal_open_blocking(app, state, request))
        .await
        .map_err(|error| {
            CommandError::system_fault(
                "terminal_open_task_failed",
                format!("Xero could not open the terminal in the background: {error}"),
            )
        })?
}

fn terminal_open_blocking<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: OpenTerminalRequestDto,
) -> CommandResult<OpenTerminalResponseDto> {
    let client_terminal_id = request
        .client_terminal_id
        .as_ref()
        .map(|value| validate_client_terminal_id(value))
        .transpose()?;

    let (cwd, transcript_target) = if let Some(project_id) = request
        .project_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let root = resolve_imported_repo_root(&app, &state, project_id)?;
        let transcript_target = client_terminal_id
            .as_deref()
            .map(|id| terminal_transcript_target(&root, id))
            .transpose()?;
        (root.to_string_lossy().into_owned(), transcript_target)
    } else {
        let cwd = dirs::home_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_owned());
        (cwd, None)
    };

    let cols = request.cols.unwrap_or(120).max(1);
    let rows = request.rows.unwrap_or(32).max(1);

    terminal_open_in_cwd(
        cwd,
        cols,
        rows,
        TerminalEventSink::tauri(app),
        transcript_target,
        !request.suppress_transcript_until_input,
    )
}

pub fn terminal_open_for_cwd(
    cwd: &Path,
    cols: Option<u16>,
    rows: Option<u16>,
) -> CommandResult<OpenTerminalResponseDto> {
    terminal_open_in_cwd(
        cwd.to_string_lossy().into_owned(),
        cols.unwrap_or(120).max(1),
        rows.unwrap_or(32).max(1),
        TerminalEventSink::none(),
        None,
        true,
    )
}

fn terminal_open_in_cwd(
    cwd: String,
    cols: u16,
    rows: u16,
    sink: TerminalEventSink,
    transcript_target: Option<TerminalTranscriptTarget>,
    transcript_enabled: bool,
) -> CommandResult<OpenTerminalResponseDto> {
    if let Some(target) = transcript_target.as_ref() {
        reset_terminal_transcript(target)?;
    }

    let shell = detect_user_shell();
    let pty_system = NativePtySystem::default();
    let pair = pty_system
        .openpty(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| {
            CommandError::system_fault(
                "terminal_open_failed",
                format!("Xero could not allocate a PTY: {error}"),
            )
        })?;

    let mut cmd = CommandBuilder::new(&shell);
    cmd.cwd(&cwd);
    if let Ok(value) = std::env::var("PATH") {
        cmd.env("PATH", value);
    }
    if let Ok(value) = std::env::var("HOME") {
        cmd.env("HOME", value);
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");

    let mut child = pair.slave.spawn_command(cmd).map_err(|error| {
        CommandError::system_fault(
            "terminal_spawn_failed",
            format!("Xero could not spawn the shell `{shell}`: {error}"),
        )
    })?;
    let shell_pid = child.process_id();
    let child_killer = child.clone_killer();
    // Drop slave handle so the child owns its end of the PTY.
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|error| {
        CommandError::system_fault(
            "terminal_reader_failed",
            format!("Xero could not duplicate the PTY reader: {error}"),
        )
    })?;
    let writer = pair.master.take_writer().map_err(|error| {
        CommandError::system_fault(
            "terminal_writer_failed",
            format!("Xero could not duplicate the PTY writer: {error}"),
        )
    })?;

    let terminal_id = format!("term-{}", current_epoch_micros());
    let started_at = now_timestamp();
    let shell_title = process_label_from_path(&shell).unwrap_or_else(|| "terminal".to_owned());

    let handle = Arc::new(TerminalHandle {
        alive: AtomicBool::new(true),
        terminal_id: terminal_id.clone(),
        shell: shell.clone(),
        cwd: cwd.clone(),
        started_at: started_at.clone(),
        title: Mutex::new(Some(shell_title)),
        next_sequence: AtomicU64::new(0),
        output: Mutex::new(TerminalOutputBuffer::new()),
        transcript_target: transcript_target.clone(),
        transcript_enabled: AtomicBool::new(transcript_enabled),
        master: Mutex::new(pair.master),
        writer: Mutex::new(writer),
        process: TerminalProcessHandle {
            child_killer: Mutex::new(child_killer),
        },
    });
    {
        let mut registry = lock_terminals()?;
        registry.insert(terminal_id.clone(), Arc::clone(&handle));
    }

    spawn_terminal_title_watcher(
        sink.clone(),
        terminal_id.clone(),
        Arc::clone(&handle),
        shell.clone(),
        shell_pid,
    );

    // Stream output to renderer-specific subscribers and retain a bounded
    // replay buffer for terminal-native clients such as dev:tui.
    let terminal_id_reader = terminal_id.clone();
    let handle_for_reader = Arc::clone(&handle);
    let sink_for_reader = sink.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    if handle_for_reader.transcript_enabled.load(Ordering::Relaxed) {
                        if let Some(target) = handle_for_reader.transcript_target.as_ref() {
                            let _ = append_terminal_transcript(target, &chunk);
                        }
                    }
                    let sequence = handle_for_reader
                        .next_sequence
                        .fetch_add(1, Ordering::Relaxed);
                    if let Ok(mut output) = handle_for_reader.output.lock() {
                        output.push(sequence, chunk.clone());
                    }
                    sink_for_reader.emit_data(TerminalDataEventPayload {
                        terminal_id: terminal_id_reader.clone(),
                        data: chunk,
                    });
                }
                Err(_) => break,
            }
        }
    });

    // Watch the child to emit an exit event and clean up the registry.
    let terminal_id_waiter = terminal_id.clone();
    let handle_for_waiter = Arc::clone(&handle);
    let sink_for_waiter = sink.clone();
    thread::spawn(move || {
        let exit_code = child
            .wait()
            .ok()
            .and_then(|status| status.exit_code().try_into().ok());
        handle_for_waiter.alive.store(false, Ordering::Relaxed);
        if let Ok(mut registry) = TERMINALS.lock() {
            registry.remove(&terminal_id_waiter);
        }
        sink_for_waiter.emit_exit(TerminalExitEventPayload {
            terminal_id: terminal_id_waiter,
            exit_code,
        });
    });

    Ok(OpenTerminalResponseDto {
        terminal_id,
        shell,
        cwd,
        started_at,
    })
}

#[tauri::command]
pub fn terminal_read_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: TerminalTranscriptRequestDto,
) -> CommandResult<TerminalTranscriptResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let client_terminal_id = validate_client_terminal_id(&request.client_terminal_id)?;
    let repo_root = resolve_imported_repo_root(&app, state.inner(), &request.project_id)?;
    let target = terminal_transcript_target(&repo_root, &client_terminal_id)?;
    let content = match fs::read_to_string(&target.path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => {
            return Err(CommandError::retryable(
                "terminal_transcript_read_failed",
                format!(
                    "Xero could not read terminal transcript `{client_terminal_id}` from app-data storage: {error}"
                ),
            ));
        }
    };
    Ok(TerminalTranscriptResponseDto {
        project_id: request.project_id,
        client_terminal_id,
        content,
    })
}

#[tauri::command]
pub fn terminal_clear_transcript<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: TerminalTranscriptRequestDto,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    let client_terminal_id = validate_client_terminal_id(&request.client_terminal_id)?;
    let repo_root = resolve_imported_repo_root(&app, state.inner(), &request.project_id)?;
    let target = terminal_transcript_target(&repo_root, &client_terminal_id)?;
    match fs::remove_file(&target.path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(CommandError::retryable(
            "terminal_transcript_clear_failed",
            format!(
                "Xero could not remove terminal transcript `{client_terminal_id}` from app-data storage: {error}"
            ),
        )),
    }
}

#[tauri::command]
pub async fn terminal_suggest<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: TerminalSuggestRequestDto,
) -> CommandResult<TerminalSuggestResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || terminal_suggest_blocking(app, state, request))
        .await
        .map_err(|error| {
            CommandError::system_fault(
                "terminal_suggest_task_failed",
                format!("Xero could not finish terminal suggestion lookup: {error}"),
            )
        })?
}

#[tauri::command]
pub fn terminal_record_command<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: TerminalRecordCommandRequestDto,
) -> CommandResult<()> {
    let project_id = require_non_empty(&request.project_id, "projectId")?.to_owned();
    let command = normalize_history_command(&request.command)
        .ok_or_else(|| CommandError::invalid_request("command"))?;
    let repo_root = resolve_imported_repo_root(&app, state.inner(), &project_id)?;
    append_terminal_history(
        &repo_root,
        TerminalHistoryEntry {
            command,
            cwd: request
                .cwd
                .and_then(|value| normalize_short_text(&value, 4096)),
            shell: request
                .shell
                .and_then(|value| normalize_short_text(&value, 256)),
            used_at: now_timestamp(),
        },
    )
}

#[tauri::command]
pub fn terminal_ignore_suggestion<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: TerminalIgnoreSuggestionRequestDto,
) -> CommandResult<()> {
    let project_id = require_non_empty(&request.project_id, "projectId")?.to_owned();
    let display = normalize_history_command(&request.display)
        .ok_or_else(|| CommandError::invalid_request("display"))?;
    let repo_root = resolve_imported_repo_root(&app, state.inner(), &project_id)?;
    append_ignored_terminal_suggestion(&repo_root, &display)
}

#[tauri::command]
pub fn terminal_write<R: Runtime>(
    _app: AppHandle<R>,
    _state: State<'_, DesktopState>,
    request: TerminalWriteRequestDto,
) -> CommandResult<()> {
    terminal_write_direct(request)
}

pub fn terminal_write_direct(request: TerminalWriteRequestDto) -> CommandResult<()> {
    let handle = {
        let registry = lock_terminals()?;
        registry.get(&request.terminal_id).cloned()
    };
    let Some(handle) = handle else {
        return Err(CommandError::user_fixable(
            "terminal_not_found",
            "This terminal has already exited.",
        ));
    };
    if handle.transcript_target.is_some() {
        handle.transcript_enabled.store(true, Ordering::Relaxed);
    }
    let mut writer = handle.writer.lock().map_err(|_| {
        CommandError::system_fault(
            "terminal_writer_poisoned",
            "Xero could not write to the terminal — internal lock poisoned.",
        )
    })?;
    writer.write_all(request.data.as_bytes()).map_err(|error| {
        CommandError::system_fault(
            "terminal_write_failed",
            format!("Xero could not write to the terminal: {error}"),
        )
    })?;
    let _ = writer.flush();
    Ok(())
}

pub fn terminal_read_buffer(
    request: TerminalReadRequestDto,
) -> CommandResult<TerminalReadResponseDto> {
    let handle = {
        let registry = lock_terminals()?;
        registry.get(&request.terminal_id).cloned()
    };
    let Some(handle) = handle else {
        return Err(CommandError::user_fixable(
            "terminal_not_found",
            "This terminal has already exited.",
        ));
    };
    let max_bytes = request
        .max_bytes
        .unwrap_or(DEFAULT_TERMINAL_READ_BYTES)
        .clamp(1, MAX_TERMINAL_READ_BYTES);
    let chunks = handle
        .output
        .lock()
        .map_err(|_| {
            CommandError::system_fault(
                "terminal_output_poisoned",
                "Xero could not read terminal output — internal lock poisoned.",
            )
        })?
        .read_after(request.after_sequence, max_bytes);
    Ok(TerminalReadResponseDto {
        terminal_id: request.terminal_id,
        running: handle.alive.load(Ordering::Relaxed),
        next_sequence: handle.next_sequence.load(Ordering::Relaxed),
        chunks,
    })
}

pub fn terminal_list_active() -> CommandResult<TerminalListResponseDto> {
    let registry = lock_terminals()?;
    let mut terminals = registry
        .values()
        .map(|handle| TerminalSummaryDto {
            terminal_id: handle.terminal_id.clone(),
            shell: handle.shell.clone(),
            cwd: handle.cwd.clone(),
            started_at: handle.started_at.clone(),
            title: handle
                .title
                .lock()
                .ok()
                .and_then(|title| title.clone())
                .unwrap_or_else(|| "terminal".to_owned()),
            running: handle.alive.load(Ordering::Relaxed),
        })
        .collect::<Vec<_>>();
    terminals.sort_by(|left, right| left.started_at.cmp(&right.started_at));
    Ok(TerminalListResponseDto { terminals })
}

#[tauri::command]
pub fn terminal_resize<R: Runtime>(
    _app: AppHandle<R>,
    _state: State<'_, DesktopState>,
    request: TerminalResizeRequestDto,
) -> CommandResult<()> {
    terminal_resize_direct(request)
}

pub fn terminal_resize_direct(request: TerminalResizeRequestDto) -> CommandResult<()> {
    let handle = {
        let registry = lock_terminals()?;
        registry.get(&request.terminal_id).cloned()
    };
    let Some(handle) = handle else {
        return Ok(());
    };
    let master = handle.master.lock().map_err(|_| {
        CommandError::system_fault(
            "terminal_master_poisoned",
            "Xero could not resize the terminal — internal lock poisoned.",
        )
    })?;
    master
        .resize(PtySize {
            rows: request.rows.max(1),
            cols: request.cols.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })
        .map_err(|error| {
            CommandError::system_fault(
                "terminal_resize_failed",
                format!("Xero could not resize the terminal: {error}"),
            )
        })?;
    Ok(())
}

#[tauri::command]
pub async fn terminal_close(request: TerminalIdRequestDto) -> CommandResult<()> {
    tauri::async_runtime::spawn_blocking(move || terminal_close_direct(request))
        .await
        .map_err(|error| {
            CommandError::system_fault(
                "terminal_close_task_failed",
                format!("Xero could not close the terminal in the background: {error}"),
            )
        })?
}

pub fn terminal_close_direct(request: TerminalIdRequestDto) -> CommandResult<()> {
    let handle = {
        let mut registry = lock_terminals()?;
        registry.remove(&request.terminal_id)
    };
    if let Some(handle) = handle {
        handle.alive.store(false, Ordering::Relaxed);
        kill_terminal_process(&handle.process);
    }
    Ok(())
}

fn kill_terminal_process(process: &TerminalProcessHandle) {
    if let Ok(mut child_killer) = process.child_killer.lock() {
        let _ = child_killer.kill();
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn require_non_empty<'a>(value: &'a str, field: &str) -> Result<&'a str, CommandError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(CommandError::user_fixable(
            "missing_required_field",
            format!("`{field}` is required."),
        ));
    }
    Ok(trimmed)
}

fn detect_user_shell() -> String {
    if cfg!(target_os = "windows") {
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".to_owned())
    } else {
        std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned())
    }
}

fn write_global_db_start_targets(
    global_db_path: &Path,
    project_id: &str,
    targets_json: &str,
) -> Result<(), CommandError> {
    let connection = open_global_database(global_db_path)?;
    connection
        .execute(
            "UPDATE projects SET start_targets = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2",
            params![targets_json, project_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "project_start_targets_persist_failed",
                format!("Xero could not persist the start targets (global DB): {error}"),
            )
        })?;
    Ok(())
}

fn write_project_db_start_targets(
    repo_root: &Path,
    project_id: &str,
    targets_json: &str,
) -> Result<(), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = project_store::open_project_database(repo_root, &database_path)?;
    connection
        .execute(
            "UPDATE projects SET start_targets = ?1, updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now') WHERE id = ?2",
            params![targets_json, project_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "project_start_targets_persist_failed",
                format!("Xero could not persist the start targets (project DB): {error}"),
            )
        })?;
    Ok(())
}

fn normalize_start_target_inputs(
    inputs: Vec<StartTargetInputDto>,
) -> Result<Vec<StartTargetDto>, CommandError> {
    let mut seen_names: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut targets: Vec<StartTargetDto> = Vec::with_capacity(inputs.len());

    for input in inputs {
        let name = input.name.trim();
        let command = input.command.trim();
        if name.is_empty() || command.is_empty() {
            continue;
        }
        let lower = name.to_ascii_lowercase();
        if !seen_names.insert(lower.clone()) {
            return Err(CommandError::user_fixable(
                "project_start_target_duplicate_name",
                format!("Start target names must be unique. `{name}` appears more than once."),
            ));
        }
        let id = input
            .id
            .as_ref()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(random_target_id);
        targets.push(StartTargetDto {
            id,
            name: name.to_owned(),
            command: command.to_owned(),
            browser_supported: input.browser_supported,
        });
    }
    Ok(targets)
}

fn random_target_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("tgt-{hex}")
}

fn terminal_suggest_blocking<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: TerminalSuggestRequestDto,
) -> CommandResult<TerminalSuggestResponseDto> {
    let project_id = require_non_empty(&request.project_id, "projectId")?.to_owned();
    let buffer = validate_suggestion_buffer(&request.buffer)?;
    let cursor = validate_suggestion_cursor(&buffer, request.cursor)?;
    let request_id = request.request_id;
    let repo_root = resolve_imported_repo_root(&app, &state, &project_id)?;
    let cwd = request
        .cwd
        .as_deref()
        .and_then(|value| normalize_existing_cwd(value, &repo_root))
        .unwrap_or_else(|| repo_root.clone());
    let ignored = read_ignored_terminal_suggestions(&repo_root);

    let mut candidates = Vec::new();
    collect_history_suggestions(&mut candidates, &repo_root, &buffer, cursor, &ignored);
    collect_shell_history_suggestions(&mut candidates, &buffer, cursor, &ignored);
    collect_path_suggestions(&mut candidates, &cwd, &buffer, cursor, &ignored);
    collect_static_command_suggestions(&mut candidates, &repo_root, &buffer, cursor, &ignored);
    rank_terminal_candidates(&mut candidates);

    let deterministic_exhausted = candidates.is_empty();
    let mut ai_attempted = false;
    if deterministic_exhausted && request.enable_ai {
        ai_attempted = true;
        if let Some(candidate) =
            suggest_terminal_ai_fallback(app, state, request, &repo_root, &buffer, cursor)
                .ok()
                .flatten()
        {
            if !ignored.contains(&candidate.display) {
                candidates.push(candidate);
            }
        }
    }

    candidates.truncate(TERMINAL_SUGGESTION_MAX_CANDIDATES);
    Ok(TerminalSuggestResponseDto {
        request_id,
        candidates,
        deterministic_exhausted,
        ai_attempted,
    })
}

fn validate_suggestion_buffer(value: &str) -> CommandResult<String> {
    if value.chars().count() > TERMINAL_SUGGESTION_MAX_BUFFER_CHARS
        || value.contains('\n')
        || value.contains('\r')
    {
        return Err(CommandError::invalid_request("buffer"));
    }
    Ok(value.to_owned())
}

fn validate_suggestion_cursor(buffer: &str, cursor: usize) -> CommandResult<usize> {
    if cursor > buffer.chars().count() {
        return Err(CommandError::invalid_request("cursor"));
    }
    Ok(cursor)
}

fn normalize_short_text(value: &str, max_chars: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(max_chars).collect())
}

fn normalize_history_command(value: &str) -> Option<String> {
    let command = value.replace(['\r', '\n'], " ");
    let command = command.split_whitespace().collect::<Vec<_>>().join(" ");
    if command.is_empty()
        || command.chars().count() > TERMINAL_SUGGESTION_MAX_COMMAND_CHARS
        || is_secret_like_command(&command)
    {
        return None;
    }
    Some(command)
}

fn normalize_existing_cwd(value: &str, repo_root: &Path) -> Option<PathBuf> {
    let path = PathBuf::from(value.trim());
    if path.is_dir() {
        return Some(path);
    }
    if value.trim().is_empty() {
        None
    } else {
        Some(repo_root.to_path_buf())
    }
}

fn terminal_suggestion_dir(repo_root: &Path) -> PathBuf {
    project_app_data_dir_for_repo(repo_root).join("terminal-suggestions")
}

fn terminal_history_path(repo_root: &Path) -> PathBuf {
    terminal_suggestion_dir(repo_root).join("history.jsonl")
}

fn terminal_ignored_path(repo_root: &Path) -> PathBuf {
    terminal_suggestion_dir(repo_root).join("ignored.jsonl")
}

fn append_terminal_history(repo_root: &Path, entry: TerminalHistoryEntry) -> CommandResult<()> {
    let path = terminal_history_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "terminal_history_dir_failed",
                format!("Xero could not create terminal history app-data storage: {error}"),
            )
        })?;
    }
    let encoded = serde_json::to_string(&entry).map_err(|error| {
        CommandError::system_fault(
            "terminal_history_encode_failed",
            format!("Xero could not encode terminal history: {error}"),
        )
    })?;
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| {
            CommandError::retryable(
                "terminal_history_open_failed",
                format!("Xero could not open terminal history app-data storage: {error}"),
            )
        })?;
    writeln!(file, "{encoded}").map_err(|error| {
        CommandError::retryable(
            "terminal_history_write_failed",
            format!("Xero could not write terminal history: {error}"),
        )
    })?;
    prune_terminal_history(&path)
}

fn prune_terminal_history(path: &Path) -> CommandResult<()> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(CommandError::retryable(
                "terminal_history_prune_read_failed",
                format!("Xero could not read terminal history for pruning: {error}"),
            ))
        }
    };
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= TERMINAL_SUGGESTION_HISTORY_LIMIT {
        return Ok(());
    }
    let kept = lines[lines.len() - TERMINAL_SUGGESTION_HISTORY_LIMIT..].join("\n") + "\n";
    fs::write(path, kept).map_err(|error| {
        CommandError::retryable(
            "terminal_history_prune_write_failed",
            format!("Xero could not prune terminal history: {error}"),
        )
    })
}

fn read_terminal_history(repo_root: &Path) -> Vec<TerminalHistoryEntry> {
    let path = terminal_history_path(repo_root);
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<TerminalHistoryEntry>(&line).ok())
        .filter(|entry| normalize_history_command(&entry.command).is_some())
        .collect()
}

fn append_ignored_terminal_suggestion(repo_root: &Path, display: &str) -> CommandResult<()> {
    let path = terminal_ignored_path(repo_root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "terminal_ignore_dir_failed",
                format!("Xero could not create terminal suggestion ignore storage: {error}"),
            )
        })?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| {
            CommandError::retryable(
                "terminal_ignore_open_failed",
                format!("Xero could not open terminal suggestion ignore storage: {error}"),
            )
        })?;
    writeln!(file, "{}", serde_json::json!({ "display": display })).map_err(|error| {
        CommandError::retryable(
            "terminal_ignore_write_failed",
            format!("Xero could not write terminal suggestion ignore storage: {error}"),
        )
    })
}

fn read_ignored_terminal_suggestions(repo_root: &Path) -> std::collections::HashSet<String> {
    let path = terminal_ignored_path(repo_root);
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return std::collections::HashSet::new(),
    };
    BufReader::new(file)
        .lines()
        .map_while(Result::ok)
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(&line).ok())
        .filter_map(|value| {
            value
                .get("display")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .collect()
}

fn collect_history_suggestions(
    candidates: &mut Vec<TerminalSuggestionCandidateDto>,
    repo_root: &Path,
    buffer: &str,
    cursor: usize,
    ignored: &std::collections::HashSet<String>,
) {
    let prefix = buffer[..byte_index_for_char(buffer, cursor)].trim_start();
    let mut seen = std::collections::HashSet::new();
    let mut history = read_terminal_history(repo_root);
    history.reverse();
    for entry in history {
        let Some(command) = normalize_history_command(&entry.command) else {
            continue;
        };
        if ignored.contains(&command) || !seen.insert(command.clone()) {
            continue;
        }
        if prefix.is_empty() || command.starts_with(prefix) {
            let replacement = command
                .strip_prefix(prefix)
                .unwrap_or(command.as_str())
                .to_owned();
            if replacement.is_empty() {
                continue;
            }
            candidates.push(make_terminal_candidate(
                replacement,
                command,
                Some("Recent command".into()),
                if prefix.is_empty() {
                    "next_command"
                } else {
                    "history"
                },
                if prefix.is_empty() { 0.74 } else { 0.92 },
                cursor,
            ));
        }
    }
}

fn collect_shell_history_suggestions(
    candidates: &mut Vec<TerminalSuggestionCandidateDto>,
    buffer: &str,
    cursor: usize,
    ignored: &std::collections::HashSet<String>,
) {
    let prefix = buffer[..byte_index_for_char(buffer, cursor)].trim_start();
    if prefix.is_empty() {
        return;
    }
    let mut seen = candidates
        .iter()
        .map(|candidate| candidate.display.clone())
        .collect::<std::collections::HashSet<_>>();
    for command in read_shell_history_candidates().into_iter().rev().take(300) {
        let Some(command) = normalize_history_command(&command) else {
            continue;
        };
        if ignored.contains(&command)
            || !seen.insert(command.clone())
            || !command.starts_with(prefix)
        {
            continue;
        }
        let replacement = command
            .strip_prefix(prefix)
            .unwrap_or(command.as_str())
            .to_owned();
        if replacement.is_empty() {
            continue;
        }
        candidates.push(make_terminal_candidate(
            replacement,
            command,
            Some("Shell history".into()),
            "shell_history",
            0.82,
            cursor,
        ));
    }
}

fn read_shell_history_candidates() -> Vec<String> {
    let Some(home) = dirs::home_dir() else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    for path in [
        home.join(".zsh_history"),
        home.join(".bash_history"),
        home.join(".local/share/fish/fish_history"),
    ] {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        for line in text.lines().rev().take(300) {
            let command = if let Some((_, command)) = line.rsplit_once(';') {
                command
            } else if let Some(command) = line.trim_start().strip_prefix("- cmd: ") {
                command
            } else {
                line
            };
            if !command.trim().is_empty() {
                commands.push(command.trim().to_owned());
            }
        }
    }
    commands
}

fn collect_path_suggestions(
    candidates: &mut Vec<TerminalSuggestionCandidateDto>,
    cwd: &Path,
    buffer: &str,
    cursor: usize,
    ignored: &std::collections::HashSet<String>,
) {
    let (token_start, token) = token_before_cursor(buffer, cursor);
    if token.is_empty() {
        return;
    }
    let command = buffer.split_whitespace().next().unwrap_or("");
    let path_context = token.contains('/')
        || token.starts_with('.')
        || matches!(
            command,
            "cd" | "ls"
                | "cat"
                | "less"
                | "tail"
                | "head"
                | "open"
                | "code"
                | "vim"
                | "nvim"
                | "rm"
                | "cp"
                | "mv"
        );
    if !path_context {
        return;
    }
    let (base_dir, name_prefix, display_prefix) = path_completion_base(cwd, token);
    let Ok(entries) = fs::read_dir(&base_dir) else {
        return;
    };
    for entry in entries.flatten().take(200) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') && !name_prefix.starts_with('.') {
            continue;
        }
        if !name.starts_with(&name_prefix) {
            continue;
        }
        let is_dir = entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false);
        let completed = format!("{display_prefix}{name}{}", if is_dir { "/" } else { "" });
        let replacement = completed
            .strip_prefix(token)
            .unwrap_or(completed.as_str())
            .to_owned();
        if replacement.is_empty() {
            continue;
        }
        let display = replace_char_range(buffer, token_start, cursor, &completed);
        if ignored.contains(&display) {
            continue;
        }
        candidates.push(make_terminal_candidate(
            replacement,
            display,
            Some(if is_dir {
                "Directory".into()
            } else {
                "File".into()
            }),
            "path",
            0.88,
            cursor,
        ));
    }
}

fn path_completion_base(cwd: &Path, token: &str) -> (PathBuf, String, String) {
    let expanded = if let Some(rest) = token.strip_prefix("~/") {
        dirs::home_dir()
            .unwrap_or_else(|| cwd.to_path_buf())
            .join(rest)
    } else if token == "~" {
        dirs::home_dir().unwrap_or_else(|| cwd.to_path_buf())
    } else {
        let path = PathBuf::from(token);
        if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        }
    };
    let base = if token.ends_with('/') {
        expanded.clone()
    } else {
        expanded.parent().unwrap_or(cwd).to_path_buf()
    };
    let prefix = if token.ends_with('/') {
        String::new()
    } else {
        expanded
            .file_name()
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default()
    };
    let display_prefix = token
        .rsplit_once('/')
        .map(|(parent, _)| format!("{parent}/"))
        .unwrap_or_default();
    (base, prefix, display_prefix)
}

fn collect_static_command_suggestions(
    candidates: &mut Vec<TerminalSuggestionCandidateDto>,
    repo_root: &Path,
    buffer: &str,
    cursor: usize,
    ignored: &std::collections::HashSet<String>,
) {
    let prefix = buffer[..byte_index_for_char(buffer, cursor)].trim_start();
    let mut commands = vec![
        ("git status", "Show working tree status", 0.80),
        ("git diff", "Review unstaged changes", 0.72),
        ("git pull", "Pull current branch", 0.70),
        ("git push", "Push current branch", 0.70),
        ("git checkout -b ", "Create a branch", 0.64),
        ("pnpm install", "Install dependencies", 0.76),
        ("pnpm dev", "Run dev script", 0.78),
        ("pnpm test", "Run tests", 0.74),
        ("npm install", "Install dependencies", 0.68),
        ("npm run dev", "Run dev script", 0.70),
        ("cargo test", "Run Rust tests", 0.74),
        ("cargo check", "Check Rust project", 0.72),
        ("cargo fmt", "Format Rust project", 0.70),
        ("ls -la", "List files", 0.62),
    ];
    let package_scripts = package_json_scripts(repo_root);
    for script in package_scripts.iter() {
        commands.push((script.as_str(), "Package script", 0.81));
    }
    for (command, description, confidence) in commands {
        if ignored.contains(command) || !command.starts_with(prefix) {
            continue;
        }
        let replacement = command.strip_prefix(prefix).unwrap_or(command).to_owned();
        if replacement.is_empty() {
            continue;
        }
        candidates.push(make_terminal_candidate(
            replacement,
            command.to_owned(),
            Some(description.to_owned()),
            "command",
            confidence,
            cursor,
        ));
    }
}

fn package_json_scripts(repo_root: &Path) -> Vec<String> {
    let text = match fs::read_to_string(repo_root.join("package.json")) {
        Ok(text) => text,
        Err(_) => return Vec::new(),
    };
    let value: serde_json::Value = match serde_json::from_str(&text) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };
    let Some(scripts) = value.get("scripts").and_then(serde_json::Value::as_object) else {
        return Vec::new();
    };
    scripts
        .keys()
        .filter(|name| {
            matches!(
                name.as_str(),
                "dev" | "test" | "build" | "lint" | "typecheck" | "check" | "preview"
            )
        })
        .map(|name| {
            if name == "dev" || name == "test" {
                format!("pnpm {name}")
            } else {
                format!("pnpm run {name}")
            }
        })
        .collect()
}

fn suggest_terminal_ai_fallback<R: Runtime + 'static>(
    app: AppHandle<R>,
    state: DesktopState,
    request: TerminalSuggestRequestDto,
    repo_root: &Path,
    buffer: &str,
    cursor: usize,
) -> CommandResult<Option<TerminalSuggestionCandidateDto>> {
    let runtime_agent_id = request
        .runtime_agent_id
        .unwrap_or_else(default_runtime_agent_id);
    let provider_profile_id = if let Some(provider_profile_id) = request
        .provider_profile_id
        .clone()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
    {
        Some(provider_profile_id)
    } else {
        resolve_requested_provider_profile_id(&app, &state, request.provider_id.as_deref())?
    };
    let controls = RuntimeRunControlInputDto {
        runtime_agent_id: runtime_agent_id.clone(),
        agent_definition_id: None,
        agent_definition_version: None,
        provider_profile_id,
        model_id: request.model_id.unwrap_or_default(),
        thinking_effort: request.thinking_effort,
        approval_mode: RuntimeRunApprovalModeDto::Yolo,
        plan_mode_required: false,
        auto_compact_enabled: false,
    };
    let provider_config = resolve_owned_agent_provider_config(&app, &state, Some(&controls))?;
    let provider = create_provider_adapter(provider_config)?;
    let provider_model_id = provider.model_id().to_owned();
    let prompt = format!(
        "Project root: {}\nCwd: {}\nTyped command prefix: {:?}\nRecent context: {}\n\nReturn one shell command completion as JSON: {{\"command\":\"...\",\"description\":\"...\"}}. The command must be a single line and must not contain secrets.",
        repo_root.display(),
        request.cwd.unwrap_or_default(),
        buffer,
        request.recent_block_context.unwrap_or_default().chars().take(600).collect::<String>(),
    );
    let turn = ProviderTurnRequest {
        system_prompt: "You suggest one safe next shell command for a developer terminal. Return only compact JSON and never include secrets, markdown, or prose.".into(),
        messages: vec![ProviderMessage::User {
            content: prompt,
            attachments: Vec::new(),
        }],
        tools: Vec::new(),
        turn_index: 0,
        controls: RuntimeRunControlStateDto {
            active: RuntimeRunActiveControlSnapshotDto {
                runtime_agent_id,
                agent_definition_id: None,
                agent_definition_version: None,
                provider_profile_id: controls.provider_profile_id.clone(),
                model_id: provider_model_id,
                thinking_effort: controls.thinking_effort.clone(),
                approval_mode: RuntimeRunApprovalModeDto::Yolo,
                plan_mode_required: false,
                auto_compact_enabled: false,
                revision: 1,
                applied_at: now_timestamp(),
            },
            pending: None,
        },
    };
    let mut emit = |_event: ProviderStreamEvent| Ok(());
    let message = match provider.stream_turn(&turn, &mut emit)? {
        ProviderTurnOutcome::Complete { message, .. } => message,
        ProviderTurnOutcome::ToolCalls { .. } => return Ok(None),
    };
    let value: serde_json::Value = match serde_json::from_str(message.trim()) {
        Ok(value) => value,
        Err(_) => return Ok(None),
    };
    let Some(command) = value
        .get("command")
        .and_then(serde_json::Value::as_str)
        .and_then(normalize_history_command)
    else {
        return Ok(None);
    };
    let prefix = buffer[..byte_index_for_char(buffer, cursor)].trim_start();
    if !prefix.is_empty() && !command.starts_with(prefix) {
        return Ok(None);
    }
    let replacement = command
        .strip_prefix(prefix)
        .unwrap_or(command.as_str())
        .to_owned();
    if replacement.is_empty() {
        return Ok(None);
    }
    Ok(Some(make_terminal_candidate(
        replacement,
        command,
        value
            .get("description")
            .and_then(serde_json::Value::as_str)
            .map(|value| value.chars().take(120).collect()),
        "ai",
        0.58,
        cursor,
    )))
}

fn rank_terminal_candidates(candidates: &mut Vec<TerminalSuggestionCandidateDto>) {
    let mut seen = std::collections::HashSet::new();
    candidates.retain(|candidate| {
        !candidate.replacement.is_empty()
            && !is_secret_like_command(&candidate.display)
            && seen.insert(candidate.display.clone())
    });
    candidates.sort_by(|a, b| {
        b.confidence
            .partial_cmp(&a.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.display.len().cmp(&b.display.len()))
    });
}

fn make_terminal_candidate(
    replacement: String,
    display: String,
    description: Option<String>,
    source: &str,
    confidence: f32,
    cursor: usize,
) -> TerminalSuggestionCandidateDto {
    TerminalSuggestionCandidateDto {
        replacement,
        display,
        description,
        source: source.to_owned(),
        confidence,
        replacement_range: TerminalSuggestionReplacementRangeDto {
            start: cursor,
            end: cursor,
        },
    }
}

fn token_before_cursor(buffer: &str, cursor: usize) -> (usize, &str) {
    let end = byte_index_for_char(buffer, cursor);
    let before = &buffer[..end];
    let start_byte = before
        .char_indices()
        .rev()
        .find(|(_, ch)| ch.is_whitespace())
        .map(|(index, ch)| index + ch.len_utf8())
        .unwrap_or(0);
    let start_char = before[..start_byte].chars().count();
    (start_char, &buffer[start_byte..end])
}

fn replace_char_range(buffer: &str, start: usize, end: usize, replacement: &str) -> String {
    let start_byte = byte_index_for_char(buffer, start);
    let end_byte = byte_index_for_char(buffer, end);
    format!(
        "{}{}{}",
        &buffer[..start_byte],
        replacement,
        &buffer[end_byte..]
    )
}

fn byte_index_for_char(value: &str, char_index: usize) -> usize {
    value
        .char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(value.len())
}

fn is_secret_like_command(command: &str) -> bool {
    let lower = command.to_ascii_lowercase();
    if [
        "password",
        "passwd",
        "passphrase",
        "secret",
        "token",
        "apikey",
        "api_key",
        "access_key",
        "private_key",
        "authorization",
        "bearer",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
    {
        return true;
    }
    lower.contains("export ")
        && (lower.contains("key=") || lower.contains("token=") || lower.contains("secret="))
}

fn validate_client_terminal_id(value: &str) -> CommandResult<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > 128
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
    {
        return Err(CommandError::invalid_request("clientTerminalId"));
    }
    Ok(trimmed.to_owned())
}

fn terminal_transcript_target(
    repo_root: &Path,
    client_terminal_id: &str,
) -> CommandResult<TerminalTranscriptTarget> {
    let client_terminal_id = validate_client_terminal_id(client_terminal_id)?;
    let digest = Sha256::digest(client_terminal_id.as_bytes());
    let filename = format!("{}.ansi", hex_digest(digest.as_slice()));
    Ok(TerminalTranscriptTarget {
        path: project_app_data_dir_for_repo(repo_root)
            .join("terminal-transcripts")
            .join(filename),
    })
}

fn append_terminal_transcript(target: &TerminalTranscriptTarget, chunk: &str) -> CommandResult<()> {
    if let Some(parent) = target.path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "terminal_transcript_dir_failed",
                format!("Xero could not create terminal transcript app-data storage: {error}"),
            )
        })?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&target.path)
        .map_err(|error| {
            CommandError::retryable(
                "terminal_transcript_append_open_failed",
                format!("Xero could not open terminal transcript app-data storage: {error}"),
            )
        })?;
    file.write_all(chunk.as_bytes()).map_err(|error| {
        CommandError::retryable(
            "terminal_transcript_append_failed",
            format!("Xero could not append terminal transcript output: {error}"),
        )
    })
}

fn reset_terminal_transcript(target: &TerminalTranscriptTarget) -> CommandResult<()> {
    if let Some(parent) = target.path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "terminal_transcript_dir_failed",
                format!("Xero could not create terminal transcript app-data storage: {error}"),
            )
        })?;
    }
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&target.path)
        .map(|_| ())
        .map_err(|error| {
            CommandError::retryable(
                "terminal_transcript_reset_failed",
                format!("Xero could not reset terminal transcript app-data storage: {error}"),
            )
        })
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn build_suggest_prompt(repo_root: &Path) -> String {
    let mut sections: Vec<String> = Vec::new();
    sections.push(format!(
        "Project root: {}",
        repo_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("(unnamed)")
    ));

    if let Some(listing) = list_top_level_entries(repo_root) {
        sections.push(format!("Top-level entries:\n{}", listing));
    }

    // Surface monorepo workspace layouts so the model can name targets per package.
    for workspace_dir in ["apps", "packages", "services", "crates"] {
        let path = repo_root.join(workspace_dir);
        if !path.is_dir() {
            continue;
        }
        if let Some(listing) = list_top_level_entries(&path) {
            sections.push(format!("=== {workspace_dir}/ ===\n{listing}"));
        }
    }

    for relative in [
        "package.json",
        "pnpm-workspace.yaml",
        "turbo.json",
        "nx.json",
        "lerna.json",
        "rush.json",
        "Cargo.toml",
        "go.work",
        "pyproject.toml",
        "Makefile",
        "Justfile",
        "justfile",
        "mprocs.yaml",
        "Procfile",
        "Procfile.dev",
        "go.mod",
        "deno.json",
        "bun.lockb",
        "bun.lock",
        "pnpm-lock.yaml",
        "yarn.lock",
        "README.md",
        "README.rst",
        "README.txt",
    ] {
        let path = repo_root.join(relative);
        if !path.is_file() {
            continue;
        }
        // Lift the cap for the root package.json so the `scripts` block isn't
        // truncated mid-way through — that's where orchestrator commands like
        // `concurrently` typically live.
        let cap = if relative == "package.json" {
            DETECTION_FILE_CAP_BYTES * 2
        } else {
            DETECTION_FILE_CAP_BYTES
        };
        if let Some(snippet) = read_file_snippet(&path, cap) {
            sections.push(format!("=== {relative} ===\n{snippet}"));
        }
    }

    sections.push(
        "Return ONLY a JSON array of {\"name\": \"...\", \"command\": \"...\"} objects.".to_owned(),
    );
    sections.join("\n\n")
}

fn list_top_level_entries(repo_root: &Path) -> Option<String> {
    let entries = std::fs::read_dir(repo_root).ok()?;
    let mut names: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                None
            } else {
                Some(name)
            }
        })
        .collect();
    names.sort();
    names.truncate(40);
    Some(names.join("\n"))
}

fn read_file_snippet(path: &Path, cap: usize) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    if text.len() <= cap {
        return Some(text);
    }
    Some(format!("{}\n…", &text[..cap]))
}

fn sanitize_suggested_targets(message: &str) -> Result<Vec<SuggestedStartTargetDto>, CommandError> {
    let body = strip_markdown_fence(message);
    let body = body.trim();
    if body.is_empty() {
        return Err(CommandError::retryable(
            "project_suggest_targets_empty",
            "The selected model returned an empty response.",
        ));
    }

    #[derive(Deserialize)]
    struct RawTarget {
        #[serde(default)]
        name: String,
        #[serde(default)]
        command: String,
        #[serde(default, rename = "browserSupported")]
        browser_supported: bool,
    }

    let raw: Vec<RawTarget> = serde_json::from_str(body).map_err(|error| {
        CommandError::retryable(
            "project_suggest_targets_invalid_json",
            format!("The selected model returned invalid JSON for start targets: {error}"),
        )
    })?;

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut targets: Vec<SuggestedStartTargetDto> = Vec::with_capacity(raw.len());
    for entry in raw {
        let name = entry.name.trim();
        let command = entry.command.trim();
        if name.is_empty() || command.is_empty() {
            continue;
        }
        let key = name.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        targets.push(SuggestedStartTargetDto {
            name: name.to_owned(),
            command: command.to_owned(),
            browser_supported: entry.browser_supported,
        });
    }

    if targets.is_empty() {
        return Err(CommandError::retryable(
            "project_suggest_targets_empty",
            "The selected model returned no usable start targets.",
        ));
    }

    Ok(targets)
}

fn strip_markdown_fence(message: &str) -> String {
    let trimmed = message.trim();
    if !trimmed.starts_with("```") {
        return trimmed.to_owned();
    }
    let mut lines: Vec<&str> = trimmed.lines().collect();
    if lines
        .first()
        .is_some_and(|line| line.trim_start().starts_with("```"))
    {
        lines.remove(0);
    }
    if lines.last().is_some_and(|line| line.trim_end() == "```") {
        lines.pop();
    }
    lines.join("\n").trim().to_owned()
}

fn current_epoch_micros() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_micros())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Result as IoResult;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Debug)]
    struct RecordingKiller {
        kills: Arc<AtomicUsize>,
    }

    impl ChildKiller for RecordingKiller {
        fn kill(&mut self) -> IoResult<()> {
            self.kills.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn clone_killer(&self) -> Box<dyn ChildKiller + Send + Sync> {
            Box::new(Self {
                kills: Arc::clone(&self.kills),
            })
        }
    }

    fn test_provider_profile(
        profile_id: &str,
        provider_id: &str,
        model_id: &str,
    ) -> crate::provider_credentials::ProviderCredentialProfile {
        crate::provider_credentials::ProviderCredentialProfile {
            profile_id: profile_id.into(),
            provider_id: provider_id.into(),
            runtime_kind: provider_id.into(),
            label: provider_id.into(),
            model_id: model_id.into(),
            preset_id: Some(provider_id.into()),
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            credential_link: None,
            updated_at: "2026-05-25T22:00:00Z".into(),
        }
    }

    #[test]
    fn terminal_process_kill_uses_independent_killer() {
        let kills = Arc::new(AtomicUsize::new(0));
        let process = TerminalProcessHandle {
            child_killer: Mutex::new(Box::new(RecordingKiller {
                kills: Arc::clone(&kills),
            })),
        };

        kill_terminal_process(&process);

        assert_eq!(kills.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn terminal_output_buffer_replays_initial_and_incremental_chunks() {
        let mut buffer = TerminalOutputBuffer::new();
        buffer.push(0, "hello".into());
        buffer.push(1, " world".into());

        let initial = buffer.read_after(None, 64);
        assert_eq!(initial.len(), 2);
        assert_eq!(initial[0].sequence, 0);
        assert_eq!(initial[0].data, "hello");

        let incremental = buffer.read_after(Some(0), 64);
        assert_eq!(incremental.len(), 1);
        assert_eq!(incremental[0].sequence, 1);
    }

    #[test]
    fn terminal_transcript_round_trips_under_project_app_data() {
        let repo = tempfile::tempdir().expect("repo");
        let target =
            terminal_transcript_target(repo.path(), "term-tab-test").expect("transcript target");

        append_terminal_transcript(&target, "first line\n").expect("append first");
        append_terminal_transcript(&target, "second line\n").expect("append second");

        let content = fs::read_to_string(&target.path).expect("read transcript");
        assert_eq!(content, "first line\nsecond line\n");
        assert!(target
            .path
            .starts_with(project_app_data_dir_for_repo(repo.path())));
        assert!(validate_client_terminal_id("../escape").is_err());
    }

    #[test]
    fn terminal_history_round_trips_under_project_app_data_and_redacts_secrets() {
        let repo = tempfile::tempdir().expect("repo");
        append_terminal_history(
            repo.path(),
            TerminalHistoryEntry {
                command: "git status".into(),
                cwd: Some(repo.path().to_string_lossy().into_owned()),
                shell: Some("/bin/zsh".into()),
                used_at: "2026-06-01T12:00:00Z".into(),
            },
        )
        .expect("append history");

        assert!(terminal_history_path(repo.path())
            .starts_with(project_app_data_dir_for_repo(repo.path())));
        assert!(normalize_history_command("export OPENAI_API_KEY=sk-test").is_none());
        assert_eq!(read_terminal_history(repo.path())[0].command, "git status");
    }

    #[test]
    fn terminal_suggestions_use_history_ranges_and_ignored_entries() {
        let repo = tempfile::tempdir().expect("repo");
        append_terminal_history(
            repo.path(),
            TerminalHistoryEntry {
                command: "git status --short".into(),
                cwd: None,
                shell: None,
                used_at: "2026-06-01T12:00:00Z".into(),
            },
        )
        .expect("append history");
        append_terminal_history(
            repo.path(),
            TerminalHistoryEntry {
                command: "git diff".into(),
                cwd: None,
                shell: None,
                used_at: "2026-06-01T12:01:00Z".into(),
            },
        )
        .expect("append history");
        let mut ignored = std::collections::HashSet::new();
        ignored.insert("git diff".to_owned());
        let mut candidates = Vec::new();

        collect_history_suggestions(&mut candidates, repo.path(), "git", 3, &ignored);

        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].display, "git status --short");
        assert_eq!(candidates[0].replacement, " status --short");
        assert_eq!(candidates[0].replacement_range.start, 3);
        assert_eq!(candidates[0].replacement_range.end, 3);
    }

    #[test]
    fn terminal_path_suggestions_complete_from_cwd() {
        let repo = tempfile::tempdir().expect("repo");
        fs::write(
            repo.path().join("Cargo.toml"),
            "[package]\nname = \"demo\"\n",
        )
        .expect("write fixture");
        let ignored = std::collections::HashSet::new();
        let mut candidates = Vec::new();

        collect_path_suggestions(&mut candidates, repo.path(), "cat Car", 7, &ignored);

        assert!(candidates.iter().any(|candidate| {
            candidate.display == "cat Cargo.toml" && candidate.replacement == "go.toml"
        }));
    }

    #[test]
    fn provider_profile_id_for_provider_uses_requested_provider() {
        let provider_profiles = ProviderCredentialsView::from_projected_profiles_for_tests(
            "openai_codex-default".into(),
            vec![
                test_provider_profile("openai_codex-default", "openai_codex", "gpt-5.4"),
                test_provider_profile("xai-default", "xai", "grok-4.3-latest"),
            ],
            Vec::new(),
        );

        let profile_id =
            provider_profile_id_for_provider(&provider_profiles, "xai").expect("profile");

        assert_eq!(profile_id.as_deref(), Some("xai-default"));
    }

    #[test]
    fn provider_profile_id_for_provider_rejects_missing_provider() {
        let provider_profiles = ProviderCredentialsView::from_projected_profiles_for_tests(
            "openai_codex-default".into(),
            vec![test_provider_profile(
                "openai_codex-default",
                "openai_codex",
                "gpt-5.4",
            )],
            Vec::new(),
        );

        let error = provider_profile_id_for_provider(&provider_profiles, "xai")
            .expect_err("missing provider should be rejected");

        assert!(format!("{error:?}").contains("provider_not_found"));
    }

    #[test]
    fn process_label_keeps_package_manager_script_context() {
        assert_eq!(
            process_label_from_args("/opt/homebrew/bin/pnpm dev").as_deref(),
            Some("pnpm dev")
        );
        assert_eq!(
            process_label_from_args("/usr/local/bin/npm run tauri:dev").as_deref(),
            Some("npm tauri:dev")
        );
    }

    #[cfg(unix)]
    #[test]
    fn ps_parser_keeps_full_argument_line() {
        let parsed = parse_ps_pid_pgid_args(
            " 12345 12340 /opt/homebrew/bin/node /path/with spaces/cli.js --flag",
        );

        assert_eq!(
            parsed,
            Some((
                12345 as libc::pid_t,
                12340 as libc::pid_t,
                "/opt/homebrew/bin/node /path/with spaces/cli.js --flag"
            ))
        );
    }

    #[test]
    fn sanitize_targets_accepts_plain_json() {
        let targets = sanitize_suggested_targets(
            r#"[{"name":"web","command":"pnpm dev"},{"name":"api","command":"cargo run"}]"#,
        )
        .expect("plain json should parse");
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].name, "web");
        assert_eq!(targets[0].command, "pnpm dev");
        assert!(!targets[0].browser_supported);
        assert_eq!(targets[1].name, "api");
    }

    #[test]
    fn sanitize_targets_preserves_browser_support_classification() {
        let targets = sanitize_suggested_targets(
            r#"[{"name":"web","command":"pnpm dev","browserSupported":true},{"name":"api","command":"cargo run","browserSupported":false}]"#,
        )
        .expect("classified json should parse");
        assert_eq!(targets.len(), 2);
        assert!(targets[0].browser_supported);
        assert!(!targets[1].browser_supported);
    }

    #[test]
    fn sanitize_targets_strips_markdown_fence() {
        let body = "```json\n[{\"name\":\"start\",\"command\":\"npm run dev\"}]\n```";
        let targets = sanitize_suggested_targets(body).expect("fenced json should parse");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "start");
    }

    #[test]
    fn sanitize_targets_drops_empty_entries_and_dedupes_by_name() {
        let body = r#"[
            {"name":"web","command":"pnpm dev"},
            {"name":"WEB","command":"duplicate"},
            {"name":"","command":"x"},
            {"name":"api","command":""}
        ]"#;
        let targets = sanitize_suggested_targets(body).expect("should accept");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "web");
    }

    #[test]
    fn sanitize_targets_errors_on_invalid_json() {
        let err = sanitize_suggested_targets("not json").expect_err("should reject");
        assert!(format!("{err:?}").contains("project_suggest_targets_invalid_json"));
    }

    #[test]
    fn sanitize_targets_errors_when_all_entries_invalid() {
        let err = sanitize_suggested_targets(r#"[{"name":"","command":""}]"#)
            .expect_err("should reject empty list");
        assert!(format!("{err:?}").contains("project_suggest_targets_empty"));
    }

    #[test]
    fn normalize_inputs_assigns_ids_to_drafts() {
        let inputs = vec![
            StartTargetInputDto {
                id: None,
                name: "web".into(),
                command: "pnpm dev".into(),
                browser_supported: true,
            },
            StartTargetInputDto {
                id: Some("tgt-existing".into()),
                name: "api".into(),
                command: "cargo run".into(),
                browser_supported: false,
            },
        ];
        let targets = normalize_start_target_inputs(inputs).expect("should normalize");
        assert_eq!(targets.len(), 2);
        assert!(targets[0].id.starts_with("tgt-"));
        assert!(targets[0].browser_supported);
        assert_eq!(targets[1].id, "tgt-existing");
    }

    #[test]
    fn normalize_inputs_rejects_duplicate_names() {
        let inputs = vec![
            StartTargetInputDto {
                id: None,
                name: "web".into(),
                command: "pnpm dev".into(),
                browser_supported: true,
            },
            StartTargetInputDto {
                id: None,
                name: "Web".into(),
                command: "echo".into(),
                browser_supported: false,
            },
        ];
        let err = normalize_start_target_inputs(inputs).expect_err("should reject");
        assert!(format!("{err:?}").contains("project_start_target_duplicate_name"));
    }

    #[test]
    fn normalize_inputs_drops_blank_rows() {
        let inputs = vec![
            StartTargetInputDto {
                id: None,
                name: "  ".into(),
                command: "x".into(),
                browser_supported: true,
            },
            StartTargetInputDto {
                id: None,
                name: "good".into(),
                command: "  ".into(),
                browser_supported: true,
            },
            StartTargetInputDto {
                id: None,
                name: "ok".into(),
                command: "ls".into(),
                browser_supported: false,
            },
        ];
        let targets = normalize_start_target_inputs(inputs).expect("should normalize");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "ok");
    }
}
