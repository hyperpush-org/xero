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

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::Command;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, LazyLock, Mutex,
};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, NativePtySystem, PtySize, PtySystem};
use rand::RngCore;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Runtime, State};

use crate::auth::now_timestamp;
use crate::commands::{
    runtime_support::{emit_project_updated, resolve_owned_agent_provider_config},
    CommandError, CommandResult, ProjectSummaryDto, ProjectUpdateReason,
    ProviderModelThinkingEffortDto, RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto,
    RuntimeRunApprovalModeDto, RuntimeRunControlInputDto, RuntimeRunControlStateDto,
    StartTargetDto,
};
use crate::db::{database_path_for_repo, project_store};
use crate::global_db::open_global_database;
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

const SUGGEST_SYSTEM_PROMPT: &str = "You suggest the shell commands a developer would run to start this project locally. Return a JSON array of {\"name\": \"...\", \"command\": \"...\"} objects and nothing else. No markdown fences, no prose, no explanation.\n\nIMPORTANT — root orchestrator detection: if the root `package.json` (or `Makefile`/`Procfile`/`mprocs.yaml`/`turbo.json` task) defines a script that fans out to multiple services in one command (via `concurrently`, `npm-run-all`, `turbo run dev`, `nx run-many`, `pnpm -r run`, `make -j`, `mprocs`, `overmind`, `foreman`, `honcho`, etc.), include it as the FIRST target named `all` (or `dev` if that matches the script name). This is the single-command \"run everything\" entry the user reaches for most often.\n\nIn addition to (not instead of) the orchestrator, for monorepos (pnpm/yarn/npm workspaces, Turborepo, Nx, Lerna, Rush, Cargo workspaces, Go workspaces) propose one target per runnable service or app so users can launch them individually. Name each per-service target after the package (e.g. `web`, `api`, `worker`) and inline a `cd <relative-path> && <cmd>` so each command runs from the project root.\n\nFor single-app projects, return one target named `start`.\n\nEach `name` must be short, lowercase, unique, and filename-safe. Each `command` must be a single line of shell.";

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
    pub cols: Option<u16>,
    #[serde(default)]
    pub rows: Option<u16>,
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

// ---------------------------------------------------------------------------
// Registry
// ---------------------------------------------------------------------------

struct TerminalHandle {
    alive: AtomicBool,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    process: TerminalProcessHandle,
}

struct TerminalProcessHandle {
    child_killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

type TerminalRegistry = HashMap<String, Arc<TerminalHandle>>;

static TERMINALS: LazyLock<Mutex<TerminalRegistry>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn lock_terminals() -> Result<std::sync::MutexGuard<'static, TerminalRegistry>, CommandError> {
    TERMINALS.lock().map_err(|_| {
        CommandError::system_fault(
            "terminal_registry_poisoned",
            "Xero could not access the terminal registry.",
        )
    })
}

fn spawn_terminal_title_watcher<R: Runtime + 'static>(
    app: AppHandle<R>,
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
                    let _ = app.emit(
                        TERMINAL_TITLE_EVENT,
                        TerminalTitleEventPayload {
                            terminal_id: terminal_id.clone(),
                            title: title.clone(),
                        },
                    );
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

fn is_script_runtime(label: &str) -> bool {
    let lower = label.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "node" | "nodejs" | "bun" | "deno" | "python" | "python3" | "ruby" | "perl"
    ) || lower.starts_with("python3.")
}

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
    emit_project_updated(&app, &repo_root, &project_id, ProjectUpdateReason::MetadataChanged)?;

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
    let runtime_agent_id = request.runtime_agent_id.unwrap_or(RuntimeAgentIdDto::Ask);

    let model_id = request
        .model_id
        .as_ref()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_default();
    let controls = RuntimeRunControlInputDto {
        runtime_agent_id,
        agent_definition_id: None,
        provider_profile_id: request
            .provider_profile_id
            .clone()
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty()),
        model_id: model_id.clone(),
        thinking_effort: request.thinking_effort.clone(),
        approval_mode: RuntimeRunApprovalModeDto::Yolo,
        plan_mode_required: false,
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
pub fn terminal_open<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: OpenTerminalRequestDto,
) -> CommandResult<OpenTerminalResponseDto> {
    let cwd = if let Some(project_id) = request
        .project_id
        .as_ref()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        let root = resolve_imported_repo_root(&app, state.inner(), project_id)?;
        root.to_string_lossy().into_owned()
    } else {
        dirs::home_dir()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_owned())
    };

    let shell = detect_user_shell();
    let cols = request.cols.unwrap_or(120).max(1);
    let rows = request.rows.unwrap_or(32).max(1);

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

    let handle = Arc::new(TerminalHandle {
        alive: AtomicBool::new(true),
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
        app.clone(),
        terminal_id.clone(),
        Arc::clone(&handle),
        shell.clone(),
        shell_pid,
    );

    // Stream output to the frontend.
    let app_for_reader = app.clone();
    let terminal_id_reader = terminal_id.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]).into_owned();
                    let _ = app_for_reader.emit(
                        TERMINAL_DATA_EVENT,
                        TerminalDataEventPayload {
                            terminal_id: terminal_id_reader.clone(),
                            data: chunk,
                        },
                    );
                }
                Err(_) => break,
            }
        }
    });

    // Watch the child to emit an exit event and clean up the registry.
    let app_for_waiter = app.clone();
    let terminal_id_waiter = terminal_id.clone();
    let handle_for_waiter = Arc::clone(&handle);
    thread::spawn(move || {
        let exit_code = child
            .wait()
            .ok()
            .and_then(|status| status.exit_code().try_into().ok());
        handle_for_waiter.alive.store(false, Ordering::Relaxed);
        if let Ok(mut registry) = TERMINALS.lock() {
            registry.remove(&terminal_id_waiter);
        }
        let _ = app_for_waiter.emit(
            TERMINAL_EXIT_EVENT,
            TerminalExitEventPayload {
                terminal_id: terminal_id_waiter,
                exit_code,
            },
        );
    });

    Ok(OpenTerminalResponseDto {
        terminal_id,
        shell,
        cwd,
        started_at,
    })
}

#[tauri::command]
pub fn terminal_write<R: Runtime>(
    _app: AppHandle<R>,
    _state: State<'_, DesktopState>,
    request: TerminalWriteRequestDto,
) -> CommandResult<()> {
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

#[tauri::command]
pub fn terminal_resize<R: Runtime>(
    _app: AppHandle<R>,
    _state: State<'_, DesktopState>,
    request: TerminalResizeRequestDto,
) -> CommandResult<()> {
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
    tauri::async_runtime::spawn_blocking(move || terminal_close_blocking(request))
        .await
        .map_err(|error| {
            CommandError::system_fault(
                "terminal_close_task_failed",
                format!("Xero could not close the terminal in the background: {error}"),
            )
        })?
}

fn terminal_close_blocking(request: TerminalIdRequestDto) -> CommandResult<()> {
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

    sections.push("Return ONLY a JSON array of {\"name\": \"...\", \"command\": \"...\"} objects.".to_owned());
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
    }

    let raw: Vec<RawTarget> = serde_json::from_str(body).map_err(|error| {
        CommandError::retryable(
            "project_suggest_targets_invalid_json",
            format!(
                "The selected model returned invalid JSON for start targets: {error}"
            ),
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
    fn process_label_detects_claude_node_wrapper() {
        let label = process_label_from_args(
            "/opt/homebrew/bin/node /Users/sn0w/.npm/_npx/123/node_modules/@anthropic-ai/claude-code/cli.js",
        );

        assert_eq!(label.as_deref(), Some("claude"));
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

    #[test]
    fn process_label_strips_shell_path_to_name() {
        assert_eq!(
            process_label_from_args("/bin/zsh -l").as_deref(),
            Some("zsh")
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
        assert_eq!(targets[1].name, "api");
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
            },
            StartTargetInputDto {
                id: Some("tgt-existing".into()),
                name: "api".into(),
                command: "cargo run".into(),
            },
        ];
        let targets = normalize_start_target_inputs(inputs).expect("should normalize");
        assert_eq!(targets.len(), 2);
        assert!(targets[0].id.starts_with("tgt-"));
        assert_eq!(targets[1].id, "tgt-existing");
    }

    #[test]
    fn normalize_inputs_rejects_duplicate_names() {
        let inputs = vec![
            StartTargetInputDto {
                id: None,
                name: "web".into(),
                command: "pnpm dev".into(),
            },
            StartTargetInputDto {
                id: None,
                name: "Web".into(),
                command: "echo".into(),
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
            },
            StartTargetInputDto {
                id: None,
                name: "good".into(),
                command: "  ".into(),
            },
            StartTargetInputDto {
                id: None,
                name: "ok".into(),
                command: "ls".into(),
            },
        ];
        let targets = normalize_start_target_inputs(inputs).expect("should normalize");
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].name, "ok");
    }
}
