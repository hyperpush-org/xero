//! Single-column app state and the event/render loop.

use std::{
    env, fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::MoveTo,
    event::{
        self, DisableBracketedPaste, EnableBracketedPaste, Event as CrosstermEvent, KeyCode,
        KeyEvent, KeyModifiers, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags,
        PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear as CrosstermClear,
        ClearType as CrosstermClearType, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend},
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Position, Rect, Size},
    text::Line,
    widgets::{Clear, Paragraph, Widget, Wrap},
    Frame, Terminal, TerminalOptions, Viewport,
};
use serde_json::{json, Value as JsonValue};

use crate::{generate_id, CliError, CliResponse, GlobalOptions};

use super::{
    composer, footer,
    palette::{self, PaletteState},
    project::ResolvedProject,
    runtime, slash, text_edit,
    text_edit::TextEdit,
    theme, transcript,
};

#[derive(Debug, Clone)]
pub struct ProviderRow {
    pub provider_id: String,
    pub default_model: String,
    pub credential_kind: String,
}

impl ProviderRow {
    /// Subscription/session-style credentials (e.g. Claude Max, OpenAI Codex
    /// login) get the "paid" highlight. Pure API-key or local-endpoint
    /// providers don't.
    pub fn is_paid_tier(&self) -> bool {
        matches!(self.credential_kind.as_str(), "app_session")
    }
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeMessageRow {
    pub role: String,
    pub content: String,
    pub attachments: Vec<RuntimeAttachmentRow>,
    pub thinking: Option<String>,
    pub tool_calls: Vec<ToolCallRow>,
}

#[derive(Debug, Clone, Default)]
pub struct RuntimeAttachmentRow {
    pub kind: String,
    pub original_name: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone)]
pub struct PendingAttachment {
    pub index: usize,
    pub source_path: PathBuf,
    pub staged: TuiStagedAttachmentDto,
}

pub(crate) struct ComposerDisplay {
    pub text: String,
    pub cursor: usize,
    pub selected_attachment: Option<(usize, usize)>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TuiStagedAttachmentDto {
    pub kind: String,
    pub absolute_path: String,
    pub media_type: String,
    pub original_name: String,
    pub size_bytes: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub width: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<i64>,
}

#[derive(Debug, Clone, Default)]
pub struct ToolCallRow {
    pub name: String,
    /// Backend-assigned id (e.g. `call_abc`). Used to join the
    /// assistant's tool call with the matching `tool_completed` event so
    /// we can show the call's elapsed time.
    pub tool_call_id: Option<String>,
    /// Short human-readable summary of the arguments — e.g. the path for
    /// `read`/`list`/`write`, the joined argv for `command`. `None` when
    /// the tool isn't recognized or arguments are missing.
    pub detail: Option<String>,
    /// `Some(duration)` once the matching `tool_completed` event has
    /// landed in the snapshot. `None` while the call is still in flight
    /// — the pill renders a spinner glyph in that case.
    pub completed_duration: Option<Duration>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RuntimeEventRow {
    pub event_kind: String,
    pub summary: String,
    pub action_id: Option<String>,
    pub safe_read_approval: bool,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct RunDetail {
    pub run_id: String,
    pub status: String,
    pub messages: Vec<RuntimeMessageRow>,
    pub events: Vec<RuntimeEventRow>,
    /// Latest streamed in-progress assistant text from the backend's
    /// `MessageDelta` events (set while the SSE is still arriving).
    /// Cleared once the assistant message is finalized into `messages`.
    pub in_progress_text: Option<String>,
    /// Latest streamed in-progress assistant reasoning / "thinking"
    /// tokens. Same lifecycle as [`in_progress_text`].
    pub in_progress_reasoning: Option<String>,
    pub tokens_used: Option<u64>,
    pub context_window: Option<u64>,
    /// When the run was started in this TUI session. Used to compute live
    /// elapsed time for tool pills that haven't completed yet.
    pub started_at: Option<Instant>,
}

/// One agent definition surfaced in the composer's mode cycler. Sourced
/// from `xero agent-definition list` — the same catalog the Tauri app
/// reads — so the TUI doesn't drift from the desktop product. Falls back
/// to a static seed when project-scoped loading is unavailable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentEntry {
    pub definition_id: String,
    pub display_name: String,
}

const DEFAULT_SELECTED_AGENT_DEFINITION_ID: &str = "generalist";

impl AgentEntry {
    pub fn label(&self) -> &str {
        if self.display_name.is_empty() {
            &self.definition_id
        } else {
            &self.display_name
        }
    }
}

fn default_agent_catalog() -> Vec<AgentEntry> {
    // Matches the built-in seeds in `client/src-tauri/src/db/migrations.rs`
    // so the offline fallback shows the same names users see in the
    // desktop UI.
    [
        ("ask", "Ask"),
        ("computer_use", "Computer Use"),
        ("plan", "Plan"),
        ("engineer", "Engineer"),
        ("debug", "Debug"),
        ("agent_create", "Agent Create"),
        ("generalist", "Agent"),
    ]
    .into_iter()
    .map(|(definition_id, display_name)| AgentEntry {
        definition_id: definition_id.into(),
        display_name: display_name.into(),
    })
    .collect()
}

fn initial_selected_agent_index(agents: &[AgentEntry], stored_agent_id: Option<&str>) -> usize {
    agents
        .iter()
        .position(|entry| entry.definition_id == DEFAULT_SELECTED_AGENT_DEFINITION_ID)
        .or_else(|| {
            stored_agent_id.and_then(|id| agents.iter().position(|entry| entry.definition_id == id))
        })
        .unwrap_or(0)
}

fn merge_missing_default_agents(mut agents: Vec<AgentEntry>) -> Vec<AgentEntry> {
    for fallback in default_agent_catalog() {
        if !agents
            .iter()
            .any(|entry| entry.definition_id == fallback.definition_id)
        {
            agents.push(fallback);
        }
    }
    agents
}

/// Mirrors `ProviderModelThinkingEffortDto` on the desktop side so cycling
/// in the TUI feeds the same downstream provider plumbing. Persisted with
/// the snake-case `x_high` spelling the rest of the codebase uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

impl ThinkingEffort {
    pub fn label(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "x_high",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::None => Self::Minimal,
            Self::Minimal => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::XHigh,
            Self::XHigh => Self::None,
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
            "none" => Self::None,
            "minimal" => Self::Minimal,
            "low" => Self::Low,
            "medium" => Self::Medium,
            "high" => Self::High,
            "x_high" | "xhigh" => Self::XHigh,
            _ => return None,
        })
    }
}

pub struct App {
    pub project: ResolvedProject,
    pub providers: Vec<ProviderRow>,
    pub selected_provider: usize,
    pub fake_provider_fixture: bool,
    pub agents: Vec<AgentEntry>,
    pub selected_agent: usize,
    pub thinking_effort: ThinkingEffort,
    pub composer: String,
    pub composer_cursor: usize,
    pub composer_desired_column: Option<usize>,
    pub slash_selected: usize,
    pub palette: Option<PaletteState>,
    pub status: Option<String>,
    pub update_notice: Option<String>,
    update_check_receiver: Option<Receiver<Result<crate::update_cli::UpdateCheck, String>>>,
    pub run_detail: Option<RunDetail>,
    pub active_run_id: Option<String>,
    pub active_session_id: Option<String>,
    pub draft_run_id: Option<String>,
    pub pending_attachments: Vec<PendingAttachment>,
    pub next_attachment_index: usize,
    pub last_ctrl_c_at: Option<std::time::Instant>,
    /// How many messages of the current run have already been written to
    /// the terminal scrollback. Anything past this index needs to be
    /// emitted via `Terminal::insert_before` on the next render.
    pub committed_messages: usize,
    /// Whether the welcome-state banner (ASCII logo) has been printed.
    /// We only emit it once per process, before the first prompt.
    pub welcome_committed: bool,
    /// Set when a command needs the host terminal purged and the welcome
    /// banner re-emitted, for example after `/new`.
    tui_reset_requested: bool,
    /// Number of rendered response rows from the current in-progress
    /// assistant message that have already been emitted to scrollback.
    /// The unfinished trailing row stays hidden until wrapping proves it
    /// has completed, so token-by-token deltas never flicker in the UI.
    pub streamed_text_rows: usize,
    /// Rendered-row cursor for the in-progress reasoning stream. Mirrors
    /// [`streamed_text_rows`] for provider reasoning / "thinking" text.
    pub streamed_reasoning_rows: usize,
    /// Current height of the inline viewport. We grow it when a run is
    /// streaming (to host the spinner + in-flight tool row) and shrink it
    /// back when the run goes idle while preserving the conversation gap.
    pub inline_height: u16,
    /// `true` when the most-recently emitted scrollback row was blank.
    /// Used to collapse adjacent blank rows from independent emit calls
    /// (e.g. a tool block's trailing blank meeting the next emit's
    /// leading blank) so paragraphs stay single-spaced.
    pub last_scrollback_line_blank: bool,
    /// Replayable model of rows already written to terminal scrollback.
    /// Terminal emulators do not reliably reflow previous output when the
    /// window is resized, so resize handling clears stale visible rows and
    /// re-emits these entries at the new width.
    committed_scrollback: Vec<ScrollbackEntry>,
    /// On-disk location for [`TuiPreferences`]. Held on the struct so test
    /// helpers can override it without touching the user's real state.
    preferences_path: PathBuf,
    /// Path to the remote-bridge identity file. Read on demand to refresh
    /// the signed-in indicator (e.g. after `/login` completes).
    identity_path: PathBuf,
    /// GitHub handle from the most recent identity read. `None` when the
    /// user is not signed in.
    pub signed_in_handle: Option<String>,
    /// `Some(flow_id)` between the moment `/login` starts an OAuth flow
    /// and the moment a follow-up `/login` finalizes it. Persisted only in
    /// memory — restarting the TUI cancels an in-flight sign-in.
    pub login_flow_id: Option<String>,
}

#[derive(Debug, Clone)]
enum ScrollbackEntry {
    Welcome,
    Message(RuntimeMessageRow),
}

impl App {
    pub fn new(globals: &GlobalOptions, project: ResolvedProject) -> Self {
        let mut providers = load_providers(globals).unwrap_or_default();
        let agents = load_agents(globals, project.project_id.as_deref())
            .unwrap_or_else(|_| default_agent_catalog());
        let preferences_path = preferences_path_for(&globals.state_dir);
        let stored = TuiPreferences::load(&preferences_path).unwrap_or_default();
        let identity_path = remote_identity_path(globals);
        let signed_in_handle = load_signed_in_handle(&identity_path);

        let selected_agent =
            initial_selected_agent_index(&agents, stored.runtime_agent_id.as_deref());
        let thinking_effort = stored
            .thinking_effort
            .as_deref()
            .and_then(ThinkingEffort::from_str)
            .unwrap_or(ThinkingEffort::High);
        let selected_provider = stored
            .provider_id
            .as_deref()
            .and_then(|id| {
                providers
                    .iter()
                    .filter(|provider| provider.provider_id != "fake_provider")
                    .position(|provider| provider.provider_id == id)
            })
            .unwrap_or(0);
        if let Some(model_id) = stored
            .model_id
            .as_deref()
            .map(str::trim)
            .filter(|model_id| !model_id.is_empty())
        {
            if let Some(index) = selected_provider_actual_index(&providers, selected_provider) {
                providers[index].default_model = model_id.to_owned();
            }
        }

        let status = if !project.registered {
            Some(
                "This directory isn't a registered Xero project. \
                 Type /register to add it, or keep going to use a scratch session."
                    .into(),
            )
        } else {
            None
        };
        Self {
            project,
            providers,
            selected_provider,
            fake_provider_fixture: false,
            agents,
            selected_agent,
            thinking_effort,
            composer: String::new(),
            composer_cursor: 0,
            composer_desired_column: None,
            slash_selected: 0,
            palette: None,
            status,
            update_notice: None,
            update_check_receiver: None,
            run_detail: None,
            active_run_id: None,
            active_session_id: None,
            draft_run_id: None,
            pending_attachments: Vec::new(),
            next_attachment_index: 1,
            last_ctrl_c_at: None,
            committed_messages: 0,
            welcome_committed: false,
            tui_reset_requested: false,
            streamed_text_rows: 0,
            streamed_reasoning_rows: 0,
            inline_height: INLINE_HEIGHT_IDLE,
            last_scrollback_line_blank: false,
            committed_scrollback: Vec::new(),
            preferences_path,
            identity_path,
            signed_in_handle,
            login_flow_id: None,
        }
    }

    /// Re-read the on-disk identity file and update the cached handle. The
    /// welcome banner shows whatever this last set, so callers like
    /// `/login` and `/logout` invoke this after the bridge mutates state.
    pub fn refresh_signed_in_handle(&mut self) {
        self.signed_in_handle = load_signed_in_handle(&self.identity_path);
    }

    pub fn start_update_check(&mut self) {
        if env::var_os("XERO_DISABLE_UPDATE_CHECK").is_some() {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let result = crate::update_cli::check_for_update(None).map_err(|error| error.message);
            let _ = sender.send(result);
        });
        self.update_check_receiver = Some(receiver);
    }

    pub fn selected_agent_label(&self) -> &str {
        self.agents
            .get(self.selected_agent)
            .map(AgentEntry::label)
            .unwrap_or("agent")
    }

    pub fn selected_agent_definition_id(&self) -> Option<&str> {
        self.agents
            .get(self.selected_agent)
            .map(|entry| entry.definition_id.as_str())
    }

    pub fn cycle_agent(&mut self) {
        if self.agents.is_empty() {
            return;
        }
        self.selected_agent = (self.selected_agent + 1) % self.agents.len();
        self.persist_preferences();
    }

    pub fn cycle_thinking_effort(&mut self) {
        self.thinking_effort = self.thinking_effort.next();
        self.persist_preferences();
    }

    pub(crate) fn replace_composer(&mut self, value: impl Into<String>) {
        self.composer = value.into();
        self.composer_cursor = self.composer.len();
        self.composer_desired_column = None;
    }

    pub(crate) fn clear_composer(&mut self) {
        self.composer.clear();
        self.composer_cursor = 0;
        self.composer_desired_column = None;
    }

    pub(crate) fn composer_cursor(&self) -> usize {
        text_edit::clamped_cursor(&self.composer, self.composer_cursor)
    }

    pub(crate) fn composer_display(&self) -> ComposerDisplay {
        display_composer(&self.project.root, &self.composer, self.composer_cursor())
    }

    pub(crate) fn composer_has_attachment_path_display(&self) -> bool {
        attachment_display_spans_from_input(&self.project.root, &self.composer)
            .is_some_and(|spans| !spans.is_empty())
    }

    /// Called whenever a persisted preference changes — the file is small
    /// so we write it eagerly rather than batching.
    fn persist_preferences(&self) {
        let preferences = TuiPreferences {
            runtime_agent_id: self.selected_agent_definition_id().map(str::to_owned),
            thinking_effort: Some(self.thinking_effort.label().to_owned()),
            provider_id: self.selected_provider_id().map(str::to_owned),
            model_id: self.selected_model_id().map(str::to_owned),
        };
        let _ = preferences.save(&self.preferences_path);
    }

    pub fn selected_provider_id(&self) -> Option<&str> {
        if self.fake_provider_fixture {
            return Some("fake_provider");
        }
        self.providers
            .iter()
            .filter(|provider| provider.provider_id != "fake_provider")
            .nth(self.selected_provider)
            .map(|provider| provider.provider_id.as_str())
    }

    pub fn selected_model_id(&self) -> Option<&str> {
        if self.fake_provider_fixture {
            return Some("fake-model");
        }
        self.providers
            .iter()
            .filter(|provider| provider.provider_id != "fake_provider")
            .nth(self.selected_provider)
            .map(|provider| provider.default_model.as_str())
    }

    pub(crate) fn select_provider_model(
        &mut self,
        provider_id: impl Into<String>,
        model_id: impl Into<String>,
        credential_kind: impl Into<String>,
    ) -> bool {
        let provider_id = provider_id.into().trim().to_owned();
        let model_id = model_id.into().trim().to_owned();
        if provider_id.is_empty() || model_id.is_empty() {
            return false;
        }
        if provider_id == "fake_provider" {
            self.fake_provider_fixture = true;
            self.persist_preferences();
            return true;
        }

        self.fake_provider_fixture = false;
        let credential_kind = credential_kind.into();
        if let Some((actual_index, selected_index)) =
            provider_position_by_id(&self.providers, &provider_id)
        {
            self.providers[actual_index].default_model = model_id.clone();
            if !credential_kind.trim().is_empty() {
                self.providers[actual_index].credential_kind = credential_kind;
            }
            self.selected_provider = selected_index;
        } else {
            self.selected_provider = self
                .providers
                .iter()
                .filter(|provider| provider.provider_id != "fake_provider")
                .count();
            self.providers.push(ProviderRow {
                provider_id,
                default_model: model_id,
                credential_kind,
            });
        }
        self.persist_preferences();
        true
    }

    /// Returns true if the active provider profile uses subscription-style
    /// credentials (Claude Max, OpenAI Codex login, etc).
    pub fn selected_is_paid_tier(&self) -> bool {
        if self.fake_provider_fixture {
            return false;
        }
        self.providers
            .iter()
            .filter(|provider| provider.provider_id != "fake_provider")
            .nth(self.selected_provider)
            .map(ProviderRow::is_paid_tier)
            .unwrap_or(false)
    }

    pub fn reset_for_new_session(&mut self, active_session_id: Option<String>) {
        self.clear_composer();
        self.slash_selected = 0;
        self.palette = None;
        self.run_detail = None;
        self.active_run_id = None;
        self.active_session_id = active_session_id;
        self.draft_run_id = None;
        self.pending_attachments.clear();
        self.next_attachment_index = 1;
        self.committed_messages = 0;
        self.welcome_committed = false;
        self.streamed_text_rows = 0;
        self.streamed_reasoning_rows = 0;
        self.last_scrollback_line_blank = false;
        self.committed_scrollback.clear();
        self.tui_reset_requested = true;
    }

    pub fn pending_attachment_bytes(&self) -> i64 {
        self.pending_attachments
            .iter()
            .map(|attachment| attachment.staged.size_bytes.max(0))
            .sum()
    }

    pub fn clear_sent_pending_attachments(&mut self) {
        self.pending_attachments.clear();
        self.draft_run_id = None;
        self.next_attachment_index = 1;
    }

    pub fn discard_pending_attachments(&mut self, globals: &GlobalOptions) -> Result<(), CliError> {
        let Some(project_id) = self.project.project_id.clone() else {
            self.clear_sent_pending_attachments();
            return Ok(());
        };
        for attachment in self.pending_attachments.clone() {
            discard_staged_attachment(globals, &project_id, &attachment.staged.absolute_path)?;
        }
        self.clear_sent_pending_attachments();
        Ok(())
    }

    fn take_tui_reset_requested(&mut self) -> bool {
        std::mem::take(&mut self.tui_reset_requested)
    }
}

fn selected_provider_actual_index(
    providers: &[ProviderRow],
    selected_provider: usize,
) -> Option<usize> {
    providers
        .iter()
        .enumerate()
        .filter(|(_, provider)| provider.provider_id != "fake_provider")
        .nth(selected_provider)
        .map(|(index, _)| index)
}

fn provider_position_by_id(providers: &[ProviderRow], provider_id: &str) -> Option<(usize, usize)> {
    let mut selected_index = 0;
    for (actual_index, provider) in providers.iter().enumerate() {
        if provider.provider_id == "fake_provider" {
            continue;
        }
        if provider.provider_id == provider_id {
            return Some((actual_index, selected_index));
        }
        selected_index += 1;
    }
    None
}

struct PromptJob {
    project_id: Option<String>,
    agent_session_id: String,
    run_id: String,
    clears_pending_attachments: bool,
    receiver: Receiver<Result<JsonValue, CliError>>,
}

pub fn run_interactive(globals: GlobalOptions) -> Result<CliResponse, CliError> {
    let project = super::project::resolve(&globals);
    let mut app = App::new(&globals, project);
    app.start_update_check();
    // Probe streaming support once so the rest of the loop can adopt the
    // NDJSON path as soon as the backend ships `--stream`.
    let _ = runtime::mode(&globals);

    enable_raw_mode().map_err(tui_io_error)?;
    let keyboard_enhancement_pushed = push_keyboard_enhancements();
    let bracketed_paste_enabled = enable_bracketed_paste();
    let _ = super::remote::ensure_started(&globals);
    if let Err(error) = ensure_active_session_for_cloud(&globals, &mut app) {
        app.status = Some(error.message);
    }
    let result = run_terminal_session(&globals, &mut app);

    let bracketed_paste_result = disable_bracketed_paste(bracketed_paste_enabled);
    let keyboard_enhancement_result = pop_keyboard_enhancements(keyboard_enhancement_pushed);
    let raw_mode_result = disable_raw_mode().map_err(tui_io_error);
    super::remote::shutdown();

    raw_mode_result?;
    result?;
    bracketed_paste_result?;
    keyboard_enhancement_result?;
    Ok(CliResponse {
        output_mode: globals.output_mode,
        text: String::new(),
        json: json!({ "kind": "tui", "status": "closed" }),
        emit: false,
    })
}

fn ensure_active_session_for_cloud(globals: &GlobalOptions, app: &mut App) -> Result<(), CliError> {
    let Some(project_id) = app.project.project_id.clone() else {
        return Ok(());
    };
    if app.active_session_id.is_none() {
        let created = invoke_json(
            globals,
            &[
                "session",
                "create",
                "--project-id",
                &project_id,
                "--title",
                "New Chat",
            ],
        )?;
        let session = created.get("session").cloned().unwrap_or(created);
        let session_id = string_field(&session, "agentSessionId");
        if session_id.trim().is_empty() {
            return Err(CliError::system_fault(
                "xero_tui_startup_session_missing_id",
                "Xero TUI created a session without an id.",
            ));
        }
        app.active_session_id = Some(session_id);
    }
    sync_active_session_to_cloud(globals, app)
}

pub(crate) fn sync_active_session_to_cloud(
    globals: &GlobalOptions,
    app: &App,
) -> Result<(), CliError> {
    let Some(project_id) = app.project.project_id.as_deref() else {
        return Ok(());
    };
    let Some(session_id) = app.active_session_id.as_deref() else {
        return Ok(());
    };
    let Some(provider_id) = app.selected_provider_id() else {
        return Ok(());
    };
    let Some(model_id) = app.selected_model_id() else {
        return Ok(());
    };
    let runtime_agent_id = app.selected_agent_definition_id().unwrap_or("generalist");
    let thinking_effort = app.thinking_effort.label();
    super::remote::publish_session_added(globals, project_id, session_id)?;
    super::remote::publish_session_controls(
        globals,
        project_id,
        session_id,
        provider_id,
        model_id,
        runtime_agent_id,
        thinking_effort,
    )
}

pub(crate) fn sync_active_session_to_cloud_best_effort(globals: &GlobalOptions, app: &App) {
    let _ = sync_active_session_to_cloud(globals, app);
}

fn auto_name_session_best_effort(globals: &GlobalOptions, project_id: &str, session_id: &str) {
    let _ = invoke_json(
        globals,
        &[
            "session",
            "auto-name",
            "--project-id",
            project_id,
            session_id,
        ],
    );
}

fn run_terminal_session(globals: &GlobalOptions, app: &mut App) -> Result<(), CliError> {
    let backend = CrosstermBackend::new(io::stdout());
    // Inline viewport: only the bottom rows belong to ratatui. Everything
    // we want to live in scrollback (welcome banner, user prompts,
    // assistant responses, tool pills) gets emitted via
    // `terminal.insert_before(...)`. The user's terminal then owns the
    // history and its native scrolling does the work.
    match Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(app.inline_height),
        },
    ) {
        Ok(mut terminal) => run_inline_terminal_session(globals, &mut terminal, app),
        Err(_) => run_fullscreen_terminal_session(globals, app),
    }
}

fn run_inline_terminal_session(
    globals: &GlobalOptions,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), CliError> {
    let result = event_loop(globals, terminal, app);
    let _ = terminal.show_cursor();
    // Drop the inline viewport so the cursor lands below it and the user
    // doesn't see the composer hanging around after we exit.
    let _ = terminal.clear().and_then(|_| terminal.flush());
    println!();
    result
}

fn run_fullscreen_terminal_session(globals: &GlobalOptions, app: &mut App) -> Result<(), CliError> {
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).map_err(tui_io_error)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = match Terminal::new(backend) {
        Ok(terminal) => terminal,
        Err(error) => {
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
            return Err(tui_io_error(error));
        }
    };

    let result = fullscreen_event_loop(globals, &mut terminal, app);
    let _ = terminal.show_cursor();
    let _ = terminal.clear().and_then(|_| terminal.flush());
    let leave_result = execute!(terminal.backend_mut(), LeaveAlternateScreen).map_err(tui_io_error);
    result?;
    leave_result
}

fn push_keyboard_enhancements() -> bool {
    if !matches!(
        crossterm::terminal::supports_keyboard_enhancement(),
        Ok(true)
    ) {
        return false;
    }
    execute!(
        io::stdout(),
        PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES
        )
    )
    .is_ok()
}

fn pop_keyboard_enhancements(pushed: bool) -> Result<(), CliError> {
    if !pushed {
        return Ok(());
    }
    execute!(io::stdout(), PopKeyboardEnhancementFlags).map_err(tui_io_error)
}

fn enable_bracketed_paste() -> bool {
    execute!(io::stdout(), EnableBracketedPaste).is_ok()
}

fn disable_bracketed_paste(enabled: bool) -> Result<(), CliError> {
    if !enabled {
        return Ok(());
    }
    execute!(io::stdout(), DisableBracketedPaste).map_err(tui_io_error)
}

/// Steady inline viewport height for normal composer states. Rebuilding an
/// inline ratatui terminal requires clearing/replaying scrollback, which reads
/// as a flash while typing. Reserving the slash-list/running rows up front
/// keeps ordinary input, `/` suggestions, and run start/finish on the cheap
/// diff-only render path.
const INLINE_HEIGHT_STEADY: u16 = 14;
pub const INLINE_HEIGHT_RUNNING: u16 = INLINE_HEIGHT_STEADY;
pub const INLINE_HEIGHT_SLASH: u16 = INLINE_HEIGHT_STEADY;
pub const INLINE_HEIGHT_IDLE: u16 = INLINE_HEIGHT_STEADY;
const CONVERSATION_COMPOSER_GAP_ROWS: u16 = 1;
// Blank row above + the `Thinking…` row. The existing conversation-to-composer
// gap below this area gives the footer its bottom breathing room.
const INLINE_THINKING_AREA_ROWS: u16 = 2;
const LOGIN_POLL_INTERVAL: Duration = Duration::from_millis(1_000);

/// Inline viewport height the next render wants. Grows for an active
/// streaming run, shrinks back when idle. When the palette is open we
/// take over the whole terminal so the overlay centers on screen and has
/// room to show its body — otherwise the dialog clamps to the bottom
/// strip and clips long messages.
fn desired_inline_height(app: &App, terminal_height: u16) -> u16 {
    if app.palette.is_some() {
        return terminal_height.max(INLINE_HEIGHT_IDLE);
    }
    desired_bottom_panel_height(app, terminal_height)
}

fn desired_bottom_panel_height(app: &App, _terminal_height: u16) -> u16 {
    let composer_required_height = composer::height(app)
        .saturating_add(CONVERSATION_COMPOSER_GAP_ROWS)
        .saturating_add(2);
    let running = matches!(
        app.run_detail.as_ref().map(|detail| detail.status.as_str()),
        Some("running")
    );
    let required_height = if running {
        composer_required_height.saturating_add(INLINE_THINKING_AREA_ROWS)
    } else {
        composer_required_height
    };
    let fixed_height = INLINE_HEIGHT_IDLE
        .max(INLINE_HEIGHT_RUNNING)
        .max(INLINE_HEIGHT_SLASH);
    fixed_height.max(required_height)
}

/// Swap in a fresh `Terminal` with a new inline viewport height when
/// the desired size has changed. Ratatui's inline viewport is fixed at
/// construction, so the only way to shrink the reserved row count is
/// to drop the old terminal and build a new one bound to the same
/// stdout backend.
fn ensure_inline_height(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), CliError> {
    let terminal_height = terminal
        .size()
        .map(|size| size.height)
        .unwrap_or(app.inline_height);
    let desired = desired_inline_height(app, terminal_height);
    if app.inline_height == desired {
        return Ok(());
    }
    reflow_terminal_view(terminal, app)?;
    Ok(())
}

fn event_loop(
    globals: &GlobalOptions,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), CliError> {
    let mut active_run: Option<PromptJob> = None;
    let mut last_login_poll_at: Option<Instant> = None;
    let mut remote_ui_events = super::remote::subscribe_ui_events();
    let mut terminal_size = current_terminal_size(terminal)?;
    loop {
        poll_update_check(app);
        poll_remote_ui_events(app, &mut remote_ui_events);
        poll_active_run(globals, app, &mut active_run);
        poll_pending_login(globals, app, &mut last_login_poll_at);
        if app.take_tui_reset_requested() {
            active_run = None;
            reset_terminal_display(terminal, app)?;
            terminal_size = current_terminal_size(terminal)?;
        }
        let damage_height = resize_damage_height(app, terminal_size.height);
        if sync_terminal_size(terminal, &mut terminal_size, damage_height)? {
            reflow_terminal_view(terminal, app)?;
            terminal_size = current_terminal_size(terminal)?;
        }
        // Grow / shrink the inline viewport to match the current run
        // state. When idle, we drop the spinner-area rows while keeping
        // the base-background gap above the composer.
        ensure_inline_height(terminal, app)?;
        // Push any new transcript content into the terminal's scrollback
        // *before* drawing the inline viewport. The terminal owns the
        // history; we only own the bottom strip.
        commit_pending_history(terminal, app)?;
        terminal
            .draw(|frame| render(frame, app))
            .map_err(tui_io_error)?;
        if !event::poll(Duration::from_millis(200)).map_err(tui_io_error)? {
            continue;
        }
        match event::read().map_err(tui_io_error)? {
            CrosstermEvent::Key(key) => match dispatch_key(app, key, &mut active_run, globals) {
                KeyOutcome::Continue => {}
                KeyOutcome::Quit => return Ok(()),
            },
            CrosstermEvent::Paste(text) => match handle_paste(app, &text, globals) {
                KeyOutcome::Continue => {}
                KeyOutcome::Quit => return Ok(()),
            },
            CrosstermEvent::Resize(_, _) => {
                let damage_height = resize_damage_height(app, terminal_size.height);
                if sync_terminal_size(terminal, &mut terminal_size, damage_height)? {
                    reflow_terminal_view(terminal, app)?;
                    terminal_size = current_terminal_size(terminal)?;
                }
            }
            _ => {}
        };
    }
}

fn fullscreen_event_loop(
    globals: &GlobalOptions,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), CliError> {
    let mut active_run: Option<PromptJob> = None;
    let mut last_login_poll_at: Option<Instant> = None;
    let mut remote_ui_events = super::remote::subscribe_ui_events();
    loop {
        poll_update_check(app);
        poll_remote_ui_events(app, &mut remote_ui_events);
        poll_active_run(globals, app, &mut active_run);
        poll_pending_login(globals, app, &mut last_login_poll_at);
        if app.take_tui_reset_requested() {
            active_run = None;
            terminal.clear().map_err(tui_io_error)?;
        }
        terminal
            .draw(|frame| render_fullscreen(frame, app))
            .map_err(tui_io_error)?;
        if !event::poll(Duration::from_millis(200)).map_err(tui_io_error)? {
            continue;
        }
        match event::read().map_err(tui_io_error)? {
            CrosstermEvent::Key(key) => match dispatch_key(app, key, &mut active_run, globals) {
                KeyOutcome::Continue => {}
                KeyOutcome::Quit => return Ok(()),
            },
            CrosstermEvent::Paste(text) => match handle_paste(app, &text, globals) {
                KeyOutcome::Continue => {}
                KeyOutcome::Quit => return Ok(()),
            },
            CrosstermEvent::Resize(_, _) => {
                terminal.autoresize().map_err(tui_io_error)?;
            }
            _ => {}
        };
    }
}

fn poll_update_check(app: &mut App) {
    let Some(receiver) = app.update_check_receiver.take() else {
        return;
    };
    match receiver.try_recv() {
        Ok(Ok(check)) => {
            app.update_notice = crate::update_cli::update_notice(&check);
        }
        Ok(Err(_)) | Err(TryRecvError::Disconnected) => {}
        Err(TryRecvError::Empty) => {
            app.update_check_receiver = Some(receiver);
        }
    }
}

fn poll_remote_ui_events(
    app: &mut App,
    remote_ui_events: &mut Receiver<super::remote::RemoteUiEvent>,
) {
    loop {
        match remote_ui_events.try_recv() {
            Ok(super::remote::RemoteUiEvent::ControlsUpdated(update)) => {
                apply_remote_controls_update(app, update);
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                *remote_ui_events = super::remote::subscribe_ui_events();
                break;
            }
        }
    }
}

fn apply_remote_controls_update(app: &mut App, update: super::remote::RemoteControlsUpdate) {
    if app.active_session_id.as_deref() != Some(update.session_id.as_str()) {
        return;
    }
    let credential_kind = app
        .providers
        .iter()
        .find(|provider| provider.provider_id == update.provider_id)
        .map(|provider| provider.credential_kind.clone())
        .unwrap_or_default();
    app.select_provider_model(update.provider_id, update.model_id, credential_kind);
    if let Some(index) = app
        .agents
        .iter()
        .position(|agent| agent.definition_id == update.runtime_agent_id)
    {
        app.selected_agent = index;
    } else if !update.runtime_agent_id.trim().is_empty() {
        app.agents.push(AgentEntry {
            display_name: update.runtime_agent_id.clone(),
            definition_id: update.runtime_agent_id,
        });
        app.selected_agent = app.agents.len().saturating_sub(1);
    }
    if let Some(thinking_effort) = ThinkingEffort::from_str(&update.thinking_effort) {
        app.thinking_effort = thinking_effort;
    }
    app.persist_preferences();
}

fn poll_pending_login(
    globals: &GlobalOptions,
    app: &mut App,
    last_login_poll_at: &mut Option<Instant>,
) {
    let Some(flow_id) = app.login_flow_id.clone() else {
        *last_login_poll_at = None;
        return;
    };
    if last_login_poll_at.is_some_and(|last_poll| last_poll.elapsed() < LOGIN_POLL_INTERVAL) {
        return;
    }

    *last_login_poll_at = Some(Instant::now());
    let relay_url = resolved_relay_url();
    match remote_bridge_for(globals).poll_github_login(&flow_id) {
        Ok(status) if status.signed_in => {
            app.login_flow_id = None;
            *last_login_poll_at = None;
            app.refresh_signed_in_handle();
            let _ = super::remote::ensure_started(globals);
            sync_active_session_to_cloud_best_effort(globals, app);
            let title = signed_in_title(app);
            app.status = Some(title.clone());
            app.palette = Some(PaletteState::Detail(auth_detail_state(
                "login",
                title,
                Vec::new(),
            )));
        }
        Ok(_) => {}
        Err(error) => {
            app.login_flow_id = None;
            *last_login_poll_at = None;
            app.palette = Some(PaletteState::Detail(auth_detail_state(
                "login",
                "GitHub login failed.".to_owned(),
                relay_error_body(&relay_url, error.to_string()),
            )));
        }
    }
}

fn reflow_terminal_view(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), CliError> {
    app.streamed_text_rows = 0;
    app.streamed_reasoning_rows = 0;
    reset_terminal_display(terminal, app)?;
    replay_committed_scrollback(terminal, app)
}

fn reset_terminal_display(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<(), CliError> {
    execute!(
        terminal.backend_mut(),
        MoveTo(0, 0),
        CrosstermClear(CrosstermClearType::Purge),
        CrosstermClear(CrosstermClearType::All)
    )
    .map_err(tui_io_error)?;
    terminal.flush().map_err(tui_io_error)?;

    let terminal_height = terminal
        .size()
        .map(|size| size.height)
        .unwrap_or(app.inline_height);
    let desired = desired_inline_height(app, terminal_height);
    let backend = CrosstermBackend::new(io::stdout());
    let mut new_terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(desired),
        },
    )
    .map_err(tui_io_error)?;
    new_terminal.clear().map_err(tui_io_error)?;
    let old = std::mem::replace(terminal, new_terminal);
    drop(old);
    app.inline_height = desired;
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalSize {
    width: u16,
    height: u16,
}

impl From<Size> for TerminalSize {
    fn from(size: Size) -> Self {
        Self {
            width: size.width,
            height: size.height,
        }
    }
}

fn current_terminal_size<B: Backend>(terminal: &Terminal<B>) -> Result<TerminalSize, CliError> {
    terminal
        .size()
        .map(TerminalSize::from)
        .map_err(tui_io_error)
}

fn sync_terminal_size<B: Backend>(
    terminal: &mut Terminal<B>,
    last_size: &mut TerminalSize,
    damage_height: u16,
) -> Result<bool, CliError> {
    let next_size = current_terminal_size(terminal)?;
    let previous_size = *last_size;
    let should_clear = next_size != previous_size
        && should_clear_previous_inline_viewport(previous_size, next_size);
    if should_clear {
        clear_inline_damage_region(terminal, previous_size, next_size, damage_height)?;
    }
    terminal.autoresize().map_err(tui_io_error)?;
    *last_size = next_size;
    if should_clear {
        clear_inline_damage_region(terminal, previous_size, next_size, damage_height)?;
    }
    Ok(next_size != previous_size)
}

fn should_clear_previous_inline_viewport(previous: TerminalSize, next: TerminalSize) -> bool {
    next.width > 0 && next.height > 0 && next != previous
}

fn resize_damage_height(app: &App, terminal_height: u16) -> u16 {
    app.inline_height
        .max(desired_inline_height(app, terminal_height))
}

fn clear_inline_damage_region<B: Backend>(
    terminal: &mut Terminal<B>,
    previous: TerminalSize,
    next: TerminalSize,
    damage_height: u16,
) -> Result<(), CliError> {
    let damage_height = damage_height.min(next.height);
    if damage_height == 0 {
        return Ok(());
    }
    let previous_top = previous.height.saturating_sub(damage_height);
    let next_top = next.height.saturating_sub(damage_height);
    let top = previous_top
        .min(next_top)
        .min(next.height.saturating_sub(1));
    let cursor = terminal
        .backend_mut()
        .get_cursor_position()
        .map_err(tui_io_error)?;
    terminal
        .backend_mut()
        .set_cursor_position(Position { x: 0, y: top })
        .map_err(tui_io_error)?;
    terminal
        .backend_mut()
        .clear_region(ClearType::CurrentLine)
        .map_err(tui_io_error)?;
    terminal
        .backend_mut()
        .clear_region(ClearType::AfterCursor)
        .map_err(tui_io_error)?;
    terminal
        .backend_mut()
        .set_cursor_position(cursor)
        .map_err(tui_io_error)?;
    terminal.backend_mut().flush().map_err(tui_io_error)
}

enum KeyOutcome {
    Continue,
    Quit,
}

fn dispatch_key(
    app: &mut App,
    key: KeyEvent,
    active_run: &mut Option<PromptJob>,
    globals: &GlobalOptions,
) -> KeyOutcome {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        let now = std::time::Instant::now();
        if let Some(previous) = app.last_ctrl_c_at {
            if now.duration_since(previous) <= Duration::from_secs(2) {
                let _ = app.discard_pending_attachments(globals);
                return KeyOutcome::Quit;
            }
        }
        app.last_ctrl_c_at = Some(now);
        app.status = Some("Press Ctrl+C again within 2s to quit.".into());
        return KeyOutcome::Continue;
    }
    app.last_ctrl_c_at = None;

    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('p') {
        if app.palette.is_none() {
            app.palette = Some(palette::open());
        }
        return KeyOutcome::Continue;
    }

    if app.palette.is_some() {
        return match palette::handle_key(app, key, globals) {
            palette::KeyResult::Continue => KeyOutcome::Continue,
            palette::KeyResult::Close { status } => {
                app.palette = None;
                if let Some(message) = status {
                    app.status = Some(message);
                }
                KeyOutcome::Continue
            }
            palette::KeyResult::Quit => {
                let _ = app.discard_pending_attachments(globals);
                KeyOutcome::Quit
            }
        };
    }

    handle_composer_key(app, key, active_run, globals)
}

fn handle_composer_key(
    app: &mut App,
    key: KeyEvent,
    active_run: &mut Option<PromptJob>,
    globals: &GlobalOptions,
) -> KeyOutcome {
    match key.code {
        KeyCode::Up if slash::is_visible(app) => {
            slash::move_selection(app, -1);
        }
        KeyCode::Down if slash::is_visible(app) => {
            slash::move_selection(app, 1);
        }
        KeyCode::Tab if slash::is_visible(app) => {
            slash::complete_selection(app);
        }
        KeyCode::Tab => {
            app.cycle_agent();
            sync_active_session_to_cloud_best_effort(globals, app);
        }
        KeyCode::BackTab => {
            app.cycle_thinking_effort();
            sync_active_session_to_cloud_best_effort(globals, app);
        }
        KeyCode::Esc => {
            if !app.composer.is_empty() {
                app.clear_composer();
                slash::reset_selection(app);
            } else if active_run.is_some() {
                app.status = Some("Interrupt is not wired yet — Esc clears input only.".into());
            }
        }
        KeyCode::Enter => {
            if key.modifiers.contains(KeyModifiers::SHIFT) {
                let edit = TextEdit::Insert('\n');
                let _ = text_edit::apply_edit_at_cursor(
                    &mut app.composer,
                    &mut app.composer_cursor,
                    &mut app.composer_desired_column,
                    edit,
                );
                slash::clamp_selection(app);
            } else {
                let raw_submission = app.composer.clone();
                let mut submission = raw_submission.trim().to_owned();
                if submission.is_empty() {
                    if !app.pending_attachments.is_empty() {
                        app.status = Some("Type a prompt to send pending attachments.".into());
                    }
                    return KeyOutcome::Continue;
                }
                if let Some(attachment_spans) =
                    attachment_file_spans_from_input(&app.project.root, &raw_submission)
                {
                    let prompt =
                        remove_attachment_spans_from_input(&raw_submission, &attachment_spans);
                    let source_paths = attachment_spans
                        .iter()
                        .map(|span| span.path.clone())
                        .collect::<Vec<_>>();
                    app.clear_composer();
                    slash::reset_selection(app);
                    match stage_source_paths(app, source_paths, globals) {
                        Ok(attached) if prompt.trim().is_empty() => {
                            app.status = Some(format_pending_attachment_summary(app, attached));
                            return KeyOutcome::Continue;
                        }
                        Ok(_) if active_run.is_some() => {
                            app.status =
                                Some("A run is already active; wait for it to finish.".into());
                            return KeyOutcome::Continue;
                        }
                        Ok(_) => match start_prompt_job(globals, app, prompt.trim()) {
                            Ok(job) => *active_run = Some(job),
                            Err(error) => app.status = Some(error.message),
                        },
                        Err(error) => app.status = Some(error.message),
                    }
                    return KeyOutcome::Continue;
                }
                if submission.starts_with('/') {
                    if let Some(selected) = slash::selected_action(app, &raw_submission) {
                        match selected {
                            slash::SelectedAction::Submit(selected) => {
                                submission = selected;
                            }
                            slash::SelectedAction::Complete(completion) => {
                                app.replace_composer(completion);
                                slash::reset_selection(app);
                                return KeyOutcome::Continue;
                            }
                        }
                    }
                }
                app.clear_composer();
                slash::reset_selection(app);
                if submission.starts_with('/') {
                    return handle_slash_command(app, &submission, globals);
                }
                if active_run.is_some() {
                    app.status = Some("A run is already active; wait for it to finish.".into());
                    return KeyOutcome::Continue;
                }
                match start_prompt_job(globals, app, &submission) {
                    Ok(job) => *active_run = Some(job),
                    Err(error) => app.status = Some(error.message),
                }
            }
        }
        _ => handle_composer_text_edit(app, key),
    }
    KeyOutcome::Continue
}

fn handle_composer_text_edit(app: &mut App, key: KeyEvent) {
    let edit = text_edit::edit_for_key(key);
    if edit == TextEdit::Backspace && remove_attachment_span_before_cursor(app) {
        slash::clamp_selection(app);
        return;
    }
    let outcome = text_edit::apply_edit_at_cursor(
        &mut app.composer,
        &mut app.composer_cursor,
        &mut app.composer_desired_column,
        edit,
    );
    if !outcome.changed {
        return;
    }
    if outcome.text_changed {
        match edit {
            TextEdit::Insert(_) => slash::reset_selection(app),
            TextEdit::Backspace
            | TextEdit::Delete
            | TextEdit::DeletePreviousWord
            | TextEdit::DeleteToLineStart => {
                slash::clamp_selection(app);
            }
            TextEdit::MoveLeft
            | TextEdit::MoveRight
            | TextEdit::MovePreviousWord
            | TextEdit::MoveNextWord
            | TextEdit::MoveUp
            | TextEdit::MoveDown
            | TextEdit::MoveToLineStart
            | TextEdit::MoveToLineEnd
            | TextEdit::Ignore => {}
        }
    } else if matches!(
        edit,
        TextEdit::MoveLeft
            | TextEdit::MoveRight
            | TextEdit::MovePreviousWord
            | TextEdit::MoveNextWord
            | TextEdit::MoveUp
            | TextEdit::MoveDown
            | TextEdit::MoveToLineStart
            | TextEdit::MoveToLineEnd
    ) && !slash::is_visible(app)
    {
        slash::clamp_selection(app);
    }
}

fn remove_attachment_span_before_cursor(app: &mut App) -> bool {
    let cursor = app.composer_cursor();
    let Some(span) = attachment_span_for_backspace(&app.project.root, &app.composer, cursor) else {
        return false;
    };
    app.composer.drain(span.start..span.end);
    app.composer_cursor = span.start;
    app.composer_desired_column = None;
    true
}

fn handle_paste(app: &mut App, text: &str, globals: &GlobalOptions) -> KeyOutcome {
    if let Some(attachment_spans) = attachment_file_spans_from_input(&app.project.root, text) {
        let prompt_fragment = remove_attachment_spans_from_input(text, &attachment_spans);
        let source_paths = attachment_spans
            .iter()
            .map(|span| span.path.clone())
            .collect::<Vec<_>>();
        match stage_source_paths(app, source_paths, globals) {
            Ok(attached) => {
                if !prompt_fragment.trim().is_empty() {
                    insert_text_into_composer(app, &prompt_fragment);
                }
                app.status = Some(format_pending_attachment_summary(app, attached));
            }
            Err(error) => app.status = Some(error.message),
        }
        return KeyOutcome::Continue;
    }
    insert_text_into_composer(app, text);
    KeyOutcome::Continue
}

fn insert_text_into_composer(app: &mut App, text: &str) {
    if text.is_empty() {
        return;
    }
    let cursor = text_edit::clamped_cursor(&app.composer, app.composer_cursor);
    app.composer.insert_str(cursor, text);
    app.composer_cursor = cursor + text.len();
    app.composer_desired_column = None;
    slash::reset_selection(app);
}

fn handle_slash_command(app: &mut App, submission: &str, globals: &GlobalOptions) -> KeyOutcome {
    let command = submission.trim_start_matches('/').trim();
    if command.is_empty() || matches!(command, "help" | "commands" | "?") {
        app.palette = Some(palette::open());
        return KeyOutcome::Continue;
    }

    let words = match parse_slash_words(command) {
        Ok(words) => words,
        Err(message) => {
            app.status = Some(message);
            return KeyOutcome::Continue;
        }
    };
    if words.is_empty() {
        app.palette = Some(palette::open());
        return KeyOutcome::Continue;
    }

    match words[0].as_str() {
        "attach" => return handle_attach_command(app, &words[1..], globals),
        "detach" => return handle_detach_command(app, &words[1..], globals),
        "attachments" => return handle_attachments_command(app),
        _ => {}
    }

    if words.len() == 1 {
        if let Some(outcome) = palette::activate_command_by_id(&words[0], app, globals) {
            return palette_result_to_key_outcome(app, outcome, globals);
        }
        if let Some(alias) = slash_dialog_alias(&words[0]) {
            if let Some(outcome) = palette::activate_command_by_id(alias, app, globals) {
                return palette_result_to_key_outcome(app, outcome, globals);
            }
        }
    }

    let args = normalize_slash_args(words);
    let args = apply_slash_scope(args, app);
    let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
    match invoke_json(globals, &borrowed) {
        Ok(value) => {
            app.palette = Some(PaletteState::Detail(palette::DetailState {
                command_id: "slash",
                title: format!("/{}", args.join(" ")),
                hint: Some("esc back   ctrl+p /commands".to_owned()),
                data: palette::DetailData::Body(palette::detail_lines_for_json(&value)),
                selected: 0,
            }));
        }
        Err(error) => {
            app.palette = Some(PaletteState::Detail(palette::DetailState {
                command_id: "slash",
                title: format!("/{}", args.join(" ")),
                hint: Some("esc back   ctrl+p /commands".to_owned()),
                data: palette::DetailData::Empty(format!("{} ({})", error.message, error.code)),
                selected: 0,
            }));
        }
    }
    KeyOutcome::Continue
}

fn handle_attach_command(app: &mut App, paths: &[String], globals: &GlobalOptions) -> KeyOutcome {
    if paths.is_empty() {
        app.status = Some("Usage: /attach <path> [path...]".into());
        return KeyOutcome::Continue;
    }
    let source_paths = paths
        .iter()
        .map(|raw_path| resolve_attachment_source_path(&app.project.root, raw_path))
        .collect::<Vec<_>>();
    attach_source_paths(app, source_paths, globals)
}

fn attach_source_paths(
    app: &mut App,
    source_paths: Vec<PathBuf>,
    globals: &GlobalOptions,
) -> KeyOutcome {
    match stage_source_paths(app, source_paths, globals) {
        Ok(attached) => app.status = Some(format_pending_attachment_summary(app, attached)),
        Err(error) => app.status = Some(error.message),
    }
    KeyOutcome::Continue
}

fn stage_source_paths(
    app: &mut App,
    source_paths: Vec<PathBuf>,
    globals: &GlobalOptions,
) -> Result<usize, CliError> {
    let Some(project_id) = app.project.project_id.clone() else {
        return Err(CliError::usage(
            "Attachments require a registered Xero project.",
        ));
    };
    if source_paths.is_empty() {
        return Err(CliError::usage("Usage: /attach <path> [path...]"));
    }

    let run_id = app
        .draft_run_id
        .get_or_insert_with(|| generate_id("tui-run"))
        .clone();
    let mut attached = 0usize;
    for source_path in source_paths {
        match stage_tui_attachment(globals, &project_id, &run_id, &source_path) {
            Ok(staged) => {
                let index = app.next_attachment_index;
                app.next_attachment_index = app.next_attachment_index.saturating_add(1);
                app.pending_attachments.push(PendingAttachment {
                    index,
                    source_path,
                    staged,
                });
                attached += 1;
            }
            Err(error) => {
                return Err(error);
            }
        }
    }
    Ok(attached)
}

fn handle_detach_command(app: &mut App, targets: &[String], globals: &GlobalOptions) -> KeyOutcome {
    if app.pending_attachments.is_empty() {
        app.status = Some("No pending attachments.".into());
        return KeyOutcome::Continue;
    }
    if targets.is_empty() {
        app.status = Some("Usage: /detach <index|name|all>".into());
        return KeyOutcome::Continue;
    }
    let target = targets.join(" ");
    if matches_normalized(&target, "all") {
        match app.discard_pending_attachments(globals) {
            Ok(()) => app.status = Some("Detached all pending attachments.".into()),
            Err(error) => app.status = Some(error.message),
        }
        return KeyOutcome::Continue;
    }
    let Some(project_id) = app.project.project_id.clone() else {
        app.clear_sent_pending_attachments();
        app.status = Some("Cleared pending attachments.".into());
        return KeyOutcome::Continue;
    };
    let position = pending_attachment_position(app, &target);
    let Some(position) = position else {
        app.status = Some(format!("No pending attachment matches `{target}`."));
        return KeyOutcome::Continue;
    };
    let attachment = app.pending_attachments[position].clone();
    match discard_staged_attachment(globals, &project_id, &attachment.staged.absolute_path) {
        Ok(()) => {
            let removed = app.pending_attachments.remove(position);
            if app.pending_attachments.is_empty() {
                app.draft_run_id = None;
                app.next_attachment_index = 1;
            }
            app.status = Some(format!("Detached {}.", removed.staged.original_name));
        }
        Err(error) => app.status = Some(error.message),
    }
    KeyOutcome::Continue
}

fn handle_attachments_command(app: &mut App) -> KeyOutcome {
    if app.pending_attachments.is_empty() {
        app.status = Some("No pending attachments.".into());
        return KeyOutcome::Continue;
    }
    app.palette = Some(PaletteState::Detail(palette::DetailState {
        command_id: "attachments",
        title: "Pending attachments".to_owned(),
        hint: Some("esc back".to_owned()),
        data: palette::DetailData::Body(
            app.pending_attachments
                .iter()
                .map(|attachment| {
                    format!(
                        "{}. {}  {}  {}",
                        attachment.index,
                        pending_attachment_display_name(attachment),
                        attachment.staged.kind,
                        attachment_size_label(attachment.staged.size_bytes)
                    )
                })
                .collect(),
        ),
        selected: 0,
    }));
    KeyOutcome::Continue
}

#[derive(Debug, Clone)]
struct ComposerToken {
    start: usize,
    end: usize,
    value: String,
}

#[derive(Debug, Clone)]
struct AttachmentPathSpan {
    start: usize,
    end: usize,
    path: PathBuf,
}

fn attachment_file_spans_from_input(
    project_root: &Path,
    input: &str,
) -> Option<Vec<AttachmentPathSpan>> {
    attachment_spans_from_input(project_root, input, true)
}

fn attachment_display_spans_from_input(
    project_root: &Path,
    input: &str,
) -> Option<Vec<AttachmentPathSpan>> {
    attachment_spans_from_input(project_root, input, false)
}

fn attachment_spans_from_input(
    project_root: &Path,
    input: &str,
    require_existing_file: bool,
) -> Option<Vec<AttachmentPathSpan>> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.contains('\n') {
        return None;
    }

    let tokens = shell_tokens_with_spans(input)?;
    let spans = attachment_spans_from_tokens(project_root, &tokens, require_existing_file);
    if !spans.is_empty() {
        return Some(spans);
    }

    whole_input_attachment_span(project_root, input, require_existing_file).map(|span| vec![span])
}

fn attachment_spans_from_tokens(
    project_root: &Path,
    tokens: &[ComposerToken],
    require_existing_file: bool,
) -> Vec<AttachmentPathSpan> {
    let mut spans = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        let Some(span) =
            attachment_span_from_token(project_root, tokens, index, require_existing_file)
        else {
            index += 1;
            continue;
        };
        let token_end = span.end;
        spans.push(span);
        index = tokens
            .iter()
            .position(|token| token.start >= token_end)
            .unwrap_or(tokens.len());
    }
    spans
}

fn attachment_span_from_token(
    project_root: &Path,
    tokens: &[ComposerToken],
    start_index: usize,
    require_existing_file: bool,
) -> Option<AttachmentPathSpan> {
    let first = tokens.get(start_index)?;
    if !looks_like_attachment_path_input(&first.value) {
        return None;
    }

    let mut value = String::new();
    for token in &tokens[start_index..] {
        if !value.is_empty() {
            value.push(' ');
        }
        value.push_str(&token.value);
        let path = resolve_attachment_source_path(project_root, &value);
        let matched = if require_existing_file {
            path.is_file()
        } else {
            looks_like_attachment_display_path(project_root, &value)
        };
        if matched {
            return Some(AttachmentPathSpan {
                start: first.start,
                end: token.end,
                path,
            });
        }
    }
    None
}

fn attachment_span_for_backspace(
    project_root: &Path,
    input: &str,
    cursor: usize,
) -> Option<AttachmentPathSpan> {
    let cursor = text_edit::clamped_cursor(input, cursor);
    let spans = attachment_display_spans_from_input(project_root, input)?;
    spans
        .into_iter()
        .find(|span| cursor > span.start && cursor <= span.end)
}

fn whole_input_attachment_span(
    project_root: &Path,
    input: &str,
    require_existing_file: bool,
) -> Option<AttachmentPathSpan> {
    let trimmed = input.trim();
    if trimmed.is_empty() || !looks_like_attachment_path_input(trimmed) {
        return None;
    }
    let path = resolve_attachment_source_path(project_root, trimmed);
    let matched = if require_existing_file {
        path.is_file()
    } else {
        looks_like_attachment_display_path(project_root, trimmed)
    };
    if !matched {
        return None;
    }
    let start = input.find(trimmed).unwrap_or(0);
    Some(AttachmentPathSpan {
        start,
        end: start + trimmed.len(),
        path,
    })
}

fn shell_tokens_with_spans(input: &str) -> Option<Vec<ComposerToken>> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut start: Option<usize> = None;
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for (index, ch) in input.char_indices() {
        if escaped {
            if start.is_none() {
                start = Some(index);
            }
            current.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            if start.is_none() {
                start = Some(index);
            }
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
        match ch {
            '"' | '\'' => {
                if start.is_none() {
                    start = Some(index);
                }
                quote = Some(ch);
            }
            ch if ch.is_whitespace() => {
                if let Some(token_start) = start.take() {
                    if !current.is_empty() {
                        tokens.push(ComposerToken {
                            start: token_start,
                            end: index,
                            value: std::mem::take(&mut current),
                        });
                    }
                }
            }
            _ => {
                if start.is_none() {
                    start = Some(index);
                }
                current.push(ch);
            }
        }
    }

    if escaped {
        current.push('\\');
    }
    if quote.is_some() {
        return None;
    }
    if let Some(token_start) = start {
        if !current.is_empty() {
            tokens.push(ComposerToken {
                start: token_start,
                end: input.len(),
                value: current,
            });
        }
    }
    Some(tokens)
}

fn display_composer(project_root: &Path, input: &str, cursor: usize) -> ComposerDisplay {
    let Some(spans) = attachment_display_spans_from_input(project_root, input) else {
        return ComposerDisplay {
            text: input.to_owned(),
            cursor: text_edit::clamped_cursor(input, cursor),
            selected_attachment: None,
        };
    };
    replace_attachment_spans_for_display(input, &spans, text_edit::clamped_cursor(input, cursor))
}

fn remove_attachment_spans_from_input(input: &str, spans: &[AttachmentPathSpan]) -> String {
    replace_attachment_spans(input, spans, false)
}

fn replace_attachment_spans_for_display(
    input: &str,
    spans: &[AttachmentPathSpan],
    cursor: usize,
) -> ComposerDisplay {
    let mut output = String::with_capacity(input.len());
    let mut last = 0;
    let mut display_cursor = None;
    let mut selected_attachment = None;
    for span in spans {
        if span.start < last || span.end > input.len() {
            continue;
        }
        if display_cursor.is_none() && cursor < span.start {
            display_cursor = Some(output.len() + cursor.saturating_sub(last));
        }
        output.push_str(&input[last..span.start]);
        let display_start = output.len();
        output.push_str(&attachment_chip_label(&span.path));
        let display_end = output.len();
        if cursor >= span.start && cursor <= span.end {
            display_cursor = Some(display_end);
            selected_attachment = Some((display_start, display_end));
        }
        last = span.end;
    }
    if display_cursor.is_none() {
        display_cursor = Some(output.len() + cursor.saturating_sub(last));
    }
    output.push_str(&input[last..]);
    ComposerDisplay {
        text: output,
        cursor: display_cursor.unwrap_or(0),
        selected_attachment,
    }
}

fn replace_attachment_spans(
    input: &str,
    spans: &[AttachmentPathSpan],
    insert_filename: bool,
) -> String {
    let mut output = String::with_capacity(input.len());
    let mut last = 0;
    for span in spans {
        if span.start < last || span.end > input.len() {
            continue;
        }
        output.push_str(&input[last..span.start]);
        if insert_filename {
            output.push_str(&path_display_name(&span.path));
        }
        last = span.end;
    }
    output.push_str(&input[last..]);
    output
}

fn looks_like_attachment_path_input(input: &str) -> bool {
    let trimmed = input.trim();
    trimmed.starts_with('/')
        || trimmed.starts_with("~/")
        || trimmed == "~"
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with('"')
        || trimmed.starts_with('\'')
}

fn looks_like_attachment_display_path(project_root: &Path, input: &str) -> bool {
    let trimmed = input.trim();
    if !looks_like_attachment_path_input(trimmed) {
        return false;
    }
    let path = resolve_attachment_source_path(project_root, trimmed);
    if path.is_file() {
        return true;
    }
    let has_directory_hint = trimmed.starts_with("~/")
        || trimmed.starts_with("./")
        || trimmed.starts_with("../")
        || trimmed.starts_with('"')
        || trimmed.starts_with('\'')
        || trimmed
            .strip_prefix('/')
            .is_some_and(|rest| rest.contains('/'));
    has_directory_hint
        && path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(looks_like_attachment_extension)
}

fn looks_like_attachment_extension(extension: &str) -> bool {
    let trimmed = extension.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 16
        && trimmed.chars().all(|ch| ch.is_ascii_alphanumeric())
        && trimmed.chars().any(|ch| ch.is_ascii_alphabetic())
}

fn attachment_chip_label(path: &Path) -> String {
    format!("[{}]", path_display_name(path))
}

fn path_display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn resolve_attachment_source_path(project_root: &Path, raw_path: &str) -> PathBuf {
    let trimmed = raw_path.trim();
    let expanded = if trimmed == "~" {
        env::var_os("HOME").map(PathBuf::from)
    } else if let Some(rest) = trimmed.strip_prefix("~/") {
        env::var_os("HOME").map(|home| PathBuf::from(home).join(rest))
    } else {
        None
    };
    let path = expanded.unwrap_or_else(|| PathBuf::from(trimmed));
    if path.is_absolute() {
        path
    } else {
        project_root.join(path)
    }
}

fn stage_tui_attachment(
    globals: &GlobalOptions,
    project_id: &str,
    run_id: &str,
    source_path: &Path,
) -> Result<TuiStagedAttachmentDto, CliError> {
    let args = vec![
        "attachment".to_owned(),
        "stage".to_owned(),
        "--project-id".to_owned(),
        project_id.to_owned(),
        "--run-id".to_owned(),
        run_id.to_owned(),
        "--path".to_owned(),
        source_path.to_string_lossy().into_owned(),
    ];
    let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
    let value = invoke_json(globals, &borrowed)?;
    let attachment = value.get("attachment").cloned().unwrap_or(value);
    serde_json::from_value(attachment).map_err(|error| CliError {
        code: "xero_tui_attachment_stage_decode_failed".into(),
        message: format!("Could not decode staged attachment: {error}"),
        exit_code: 1,
    })
}

fn discard_staged_attachment(
    globals: &GlobalOptions,
    project_id: &str,
    absolute_path: &str,
) -> Result<(), CliError> {
    let args = vec![
        "attachment".to_owned(),
        "discard".to_owned(),
        "--project-id".to_owned(),
        project_id.to_owned(),
        "--absolute-path".to_owned(),
        absolute_path.to_owned(),
    ];
    let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
    invoke_json(globals, &borrowed).map(|_| ())
}

fn pending_attachment_position(app: &App, target: &str) -> Option<usize> {
    if let Ok(index) = target.parse::<usize>() {
        return app
            .pending_attachments
            .iter()
            .position(|attachment| attachment.index == index);
    }
    app.pending_attachments.iter().position(|attachment| {
        attachment.staged.original_name == target
            || attachment
                .source_path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name == target)
    })
}

fn pending_attachment_display_name(attachment: &PendingAttachment) -> String {
    if !attachment.staged.original_name.trim().is_empty() {
        return attachment.staged.original_name.clone();
    }
    attachment
        .source_path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_owned)
        .unwrap_or_else(|| attachment.source_path.display().to_string())
}

fn matches_normalized(value: &str, expected: &str) -> bool {
    value
        .trim()
        .replace('-', "_")
        .eq_ignore_ascii_case(expected)
}

fn format_pending_attachment_summary(app: &App, added: usize) -> String {
    format!(
        "Attached {added}; pending {} file(s), {} total.",
        app.pending_attachments.len(),
        attachment_size_label(app.pending_attachment_bytes())
    )
}

pub(crate) fn attachment_size_label(size_bytes: i64) -> String {
    let bytes = size_bytes.max(0) as f64;
    if bytes < 1024.0 {
        return format!("{} B", size_bytes.max(0));
    }
    let kib = bytes / 1024.0;
    if kib < 1024.0 {
        return format!("{kib:.1} KB");
    }
    let mib = kib / 1024.0;
    format!("{mib:.1} MB")
}

/// Driver for `/login` — starts the OAuth flow on first call. The event
/// loop polls pending flows automatically, and a follow-up `/login` still
/// forces an immediate poll for keyboard users.
pub(crate) fn run_login_step(globals: &GlobalOptions, app: &mut App) -> palette::OpenOutcome {
    let relay_url = resolved_relay_url();
    let bridge = remote_bridge_for(globals);
    if let Some(flow_id) = app.login_flow_id.clone() {
        match bridge.poll_github_login(&flow_id) {
            Ok(status) if status.signed_in => {
                app.login_flow_id = None;
                app.refresh_signed_in_handle();
                let title = signed_in_title(app);
                return auth_detail("login", title, Vec::new());
            }
            Ok(_) => {
                return auth_detail(
                    "login",
                    "Still waiting for GitHub.".to_owned(),
                    vec![
                        "Finish signing in in your browser. Xero will complete this automatically."
                            .to_owned(),
                    ],
                );
            }
            Err(error) => {
                app.login_flow_id = None;
                return auth_detail(
                    "login",
                    "GitHub login failed.".to_owned(),
                    relay_error_body(&relay_url, error.to_string()),
                );
            }
        }
    }

    match bridge.sign_in_with_github() {
        Ok(status) if status.signed_in => {
            app.login_flow_id = None;
            app.refresh_signed_in_handle();
            let _ = super::remote::ensure_started(globals);
            sync_active_session_to_cloud_best_effort(globals, app);
            let title = signed_in_title(app);
            auth_detail("login", title, Vec::new())
        }
        Ok(status) => {
            app.login_flow_id = status.flow_id.clone();
            let url = status.authorization_url.unwrap_or_default();
            if !url.is_empty() {
                open_in_browser(&url);
            }
            let mut body = Vec::new();
            if !url.is_empty() {
                body.push("Open this URL to sign in:".to_owned());
                body.push(url);
                body.push(String::new());
            }
            body.push("Waiting for GitHub to finish sign-in.".to_owned());
            auth_detail("login", "Sign in with GitHub".to_owned(), body)
        }
        Err(error) => auth_detail(
            "login",
            "Could not start GitHub login.".to_owned(),
            relay_error_body(&relay_url, error.to_string()),
        ),
    }
}

fn signed_in_title(app: &App) -> String {
    match app.signed_in_handle.as_deref() {
        Some(handle) => format!("Signed in as {handle}."),
        None => "Signed in.".to_owned(),
    }
}

fn resolved_relay_url() -> String {
    xero_remote_bridge::BridgeConfig::from_env_or_local("Xero TUI").relay_url
}

fn relay_error_body(relay_url: &str, error: String) -> Vec<String> {
    vec![
        error,
        String::new(),
        format!("Relay: {relay_url}"),
        "Start the Xero relay with `pnpm run dev:server`, or set XERO_REMOTE_RELAY_URL/VITE_XERO_SERVER_URL to a reachable relay before retrying.".to_owned(),
    ]
}

/// Driver for `/logout` — clears the relay session and refreshes the
/// cached handle so the welcome banner stops showing the old identity.
pub(crate) fn run_logout_step(globals: &GlobalOptions, app: &mut App) -> palette::OpenOutcome {
    let relay_url = resolved_relay_url();
    let bridge = remote_bridge_for(globals);
    match bridge.sign_out() {
        Ok(()) => {
            app.login_flow_id = None;
            super::remote::shutdown();
            app.refresh_signed_in_handle();
            auth_detail("logout", "Signed out of GitHub.".to_owned(), Vec::new())
        }
        Err(error) => auth_detail(
            "logout",
            "Logout failed.".to_owned(),
            relay_error_body(&relay_url, error.to_string()),
        ),
    }
}

fn auth_detail(command_id: &'static str, title: String, body: Vec<String>) -> palette::OpenOutcome {
    palette::OpenOutcome::Detail(auth_detail_state(command_id, title, body))
}

fn auth_detail_state(
    command_id: &'static str,
    title: String,
    body: Vec<String>,
) -> palette::DetailState {
    let data = if body.is_empty() {
        palette::DetailData::Empty(title.clone())
    } else {
        palette::DetailData::Body(body)
    };
    palette::DetailState {
        command_id,
        title,
        hint: Some("esc back".to_owned()),
        data,
        selected: 0,
    }
}

fn remote_bridge_for(
    globals: &GlobalOptions,
) -> xero_remote_bridge::RemoteBridge<xero_remote_bridge::FileIdentityStore> {
    let remote_dir = crate::cli_app_data_root(globals).join("remote");
    xero_remote_bridge::RemoteBridge::new(
        xero_remote_bridge::BridgeConfig::from_env_or_local("Xero TUI"),
        xero_remote_bridge::FileIdentityStore::new(remote_dir.join("desktop-identity.json")),
    )
}

fn open_in_browser(url: &str) {
    // Best-effort — the URL is also surfaced in the palette so the user
    // can copy it manually if the platform helper isn't on PATH.
    #[cfg(target_os = "macos")]
    let command = ("open", vec![url]);
    #[cfg(target_os = "linux")]
    let command = ("xdg-open", vec![url]);
    #[cfg(target_os = "windows")]
    let command = ("cmd", vec!["/C", "start", "", url]);
    let _ = std::process::Command::new(command.0)
        .args(&command.1)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

fn palette_result_to_key_outcome(
    app: &mut App,
    result: palette::KeyResult,
    globals: &GlobalOptions,
) -> KeyOutcome {
    match result {
        palette::KeyResult::Continue => KeyOutcome::Continue,
        palette::KeyResult::Close { status } => {
            app.palette = None;
            if let Some(message) = status {
                app.status = Some(message);
            }
            KeyOutcome::Continue
        }
        palette::KeyResult::Quit => {
            let _ = app.discard_pending_attachments(globals);
            KeyOutcome::Quit
        }
    }
}

pub(crate) fn slash_dialog_alias(command: &str) -> Option<&'static str> {
    Some(match command {
        "session" | "sessions" => "sessions",
        "provider" | "providers" => "providers",
        "model" | "models" => "model",
        "agent" | "agents" | "agent-definitions" => "agents",
        "skill" | "skills" | "plugin" | "plugins" => "skills",
        "process" | "processes" | "runner" => "processes",
        "setting" | "settings" => "settings",
        "auth" => "auth",
        "event" | "events" => "events",
        "context" => "context",
        "recovery" => "recovery",
        "usage" => "usage",
        "file" | "files" => "files",
        "git" => "git",
        _ => return None,
    })
}

fn normalize_slash_args(mut words: Vec<String>) -> Vec<String> {
    if words.first().is_some_and(|word| word == "xero") {
        words.remove(0);
    }
    match words.as_slice() {
        [only] if only == "sessions" => vec!["session".into(), "list".into()],
        [only] if only == "providers" => vec!["provider".into(), "list".into()],
        [only] if only == "agents" => vec!["agent-definition".into(), "list".into()],
        [only] if only == "skills" => vec!["skills".into(), "list".into()],
        [only] if only == "plugins" => vec!["plugins".into(), "list".into()],
        [only] if only == "processes" => vec!["process".into(), "list".into()],
        [only] if only == "settings" => vec!["settings".into(), "agent-tooling".into()],
        [only] if only == "auth" => vec!["provider".into(), "list".into()],
        _ => words,
    }
}

fn apply_slash_scope(mut args: Vec<String>, app: &App) -> Vec<String> {
    let root = args.first().map(String::as_str);
    if matches!(
        root,
        Some(
            "agent-definition"
                | "agent-definitions"
                | "conversation"
                | "notification"
                | "process"
                | "runner"
                | "project-state"
                | "session"
                | "usage"
        )
    ) {
        append_project_id_if_missing(&mut args, app);
    } else if matches!(root, Some("file" | "git" | "workspace")) {
        append_project_or_repo_if_missing(&mut args, app);
    } else if matches!(root, Some("skills" | "skill" | "plugins" | "plugin")) {
        append_project_id_if_available(&mut args, app);
    }
    args
}

fn append_project_id_if_missing(args: &mut Vec<String>, app: &App) {
    if has_slash_option(args, "--project-id") {
        return;
    }
    if let Some(project_id) = app.project.project_id.as_deref() {
        args.push("--project-id".into());
        args.push(project_id.to_owned());
    }
}

fn append_project_id_if_available(args: &mut Vec<String>, app: &App) {
    if has_slash_option(args, "--project-id") {
        return;
    }
    if let Some(project_id) = app.project.project_id.as_deref() {
        args.push("--project-id".into());
        args.push(project_id.to_owned());
    }
}

fn append_project_or_repo_if_missing(args: &mut Vec<String>, app: &App) {
    if has_slash_option(args, "--project-id") || has_slash_option(args, "--repo") {
        return;
    }
    if let Some(project_id) = app.project.project_id.as_deref() {
        args.push("--project-id".into());
        args.push(project_id.to_owned());
    } else if let Some(root) = app.project.root.to_str() {
        args.push("--repo".into());
        args.push(root.to_owned());
    }
}

fn has_slash_option(args: &[String], option: &str) -> bool {
    let prefix = format!("{option}=");
    args.iter()
        .any(|arg| arg == option || arg.starts_with(prefix.as_str()))
}

fn parse_slash_words(command: &str) -> Result<Vec<String>, String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut escaped = false;

    for ch in command.chars() {
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
        match ch {
            '"' | '\'' => quote = Some(ch),
            ch if ch.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            _ => current.push(ch),
        }
    }

    if escaped {
        current.push('\\');
    }
    if let Some(active_quote) = quote {
        return Err(format!("Unclosed quote `{active_quote}` in slash command."));
    }
    if !current.is_empty() {
        words.push(current);
    }
    Ok(words)
}

fn start_prompt_job(
    globals: &GlobalOptions,
    app: &mut App,
    prompt: &str,
) -> Result<PromptJob, CliError> {
    let provider_id = app
        .selected_provider_id()
        .ok_or_else(|| {
            CliError::usage(
                "No provider available. Open the palette (Ctrl+P) to pick one once it lands.",
            )
        })?
        .to_owned();
    let model_id = app
        .selected_model_id()
        .map(str::to_owned)
        .unwrap_or_else(|| "fake-model".into());
    let runtime_agent_id = app
        .selected_agent_definition_id()
        .unwrap_or("engineer")
        .to_owned();
    let thinking_effort = app.thinking_effort.label().to_owned();
    let run_id = app
        .draft_run_id
        .clone()
        .unwrap_or_else(|| generate_id("tui-run"));
    let project_id = app.project.project_id.clone();
    let agent_session_id = app
        .active_session_id
        .clone()
        .unwrap_or_else(|| generate_id("session"));
    let pending_attachments = app.pending_attachments.clone();
    let mut owned = vec![
        "agent".to_owned(),
        "exec".to_owned(),
        "--provider".to_owned(),
        provider_id.clone(),
        "--model".to_owned(),
        model_id,
        "--run-id".to_owned(),
        run_id.clone(),
        "--runtime-agent-id".to_owned(),
        runtime_agent_id.clone(),
        "--agent-definition-id".to_owned(),
        runtime_agent_id.clone(),
        "--thinking-effort".to_owned(),
        thinking_effort,
        "--prompt".to_owned(),
        prompt.to_owned(),
    ];
    if let Some(project_id) = project_id.as_deref() {
        owned.push("--project-id".into());
        owned.push(project_id.to_owned());
    }
    owned.push("--session-id".into());
    owned.push(agent_session_id.clone());
    if !pending_attachments.is_empty() {
        let attachments_json = serde_json::to_string(
            &pending_attachments
                .iter()
                .map(|attachment| attachment.staged.clone())
                .collect::<Vec<_>>(),
        )
        .map_err(|error| CliError::system_fault("xero_tui_attachment_encode", error.to_string()))?;
        owned.push("--attachments-json".into());
        owned.push(attachments_json);
    }

    let (sender, receiver) = mpsc::channel();
    let globals_clone = globals.clone();
    thread::spawn(move || {
        let borrowed = owned.iter().map(String::as_str).collect::<Vec<_>>();
        let _ = sender.send(invoke_json(&globals_clone, &borrowed));
    });

    app.status = None;
    app.active_session_id = Some(agent_session_id.clone());
    let started_at = Instant::now();
    app.run_detail = Some(RunDetail {
        run_id: run_id.clone(),
        status: "running".into(),
        messages: vec![RuntimeMessageRow {
            role: "user".into(),
            content: prompt.to_owned(),
            attachments: pending_attachments
                .iter()
                .map(|attachment| RuntimeAttachmentRow {
                    kind: attachment.staged.kind.clone(),
                    original_name: attachment.staged.original_name.clone(),
                    size_bytes: attachment.staged.size_bytes,
                })
                .collect(),
            thinking: None,
            tool_calls: Vec::new(),
        }],
        events: Vec::new(),
        in_progress_text: None,
        in_progress_reasoning: None,
        tokens_used: None,
        context_window: None,
        started_at: Some(started_at),
    });
    app.active_run_id = Some(run_id.clone());
    // A new run resets the "what's been committed to scrollback" counter
    // so the fresh user prompt + assistant response get emitted.
    app.committed_messages = 0;
    app.streamed_text_rows = 0;
    app.streamed_reasoning_rows = 0;
    app.last_scrollback_line_blank = false;

    Ok(PromptJob {
        project_id,
        agent_session_id,
        run_id,
        clears_pending_attachments: !pending_attachments.is_empty(),
        receiver,
    })
}

fn poll_active_run(globals: &GlobalOptions, app: &mut App, active_run: &mut Option<PromptJob>) {
    let Some(job) = active_run.as_mut() else {
        return;
    };
    let started_at = app.run_detail.as_ref().and_then(|detail| detail.started_at);
    if let Some(project_id) = job.project_id.as_deref() {
        if let Ok(mut detail) = load_run_detail(globals, project_id, &job.run_id) {
            detail.started_at = started_at;
            app.run_detail = Some(detail);
        }
    }
    match job.receiver.try_recv() {
        Ok(Ok(value)) => {
            let snapshot = value.get("snapshot").cloned().unwrap_or(value);
            let mut detail = run_detail_from_snapshot(&snapshot);
            detail.started_at = started_at;
            app.run_detail = Some(detail);
            let publish_project_id = snapshot
                .get("projectId")
                .and_then(JsonValue::as_str)
                .filter(|project_id| !project_id.trim().is_empty())
                .or(job.project_id.as_deref());
            let publish_session_id = snapshot
                .get("agentSessionId")
                .and_then(JsonValue::as_str)
                .filter(|session_id| !session_id.trim().is_empty())
                .unwrap_or(&job.agent_session_id);
            app.active_session_id = Some(publish_session_id.to_owned());
            if let Some(project_id) = publish_project_id {
                auto_name_session_best_effort(globals, project_id, publish_session_id);
                let _ =
                    super::remote::publish_session_added(globals, project_id, publish_session_id);
                let _ = super::remote::publish_session_snapshot_with_run(
                    globals,
                    project_id,
                    publish_session_id,
                    Some(&snapshot),
                );
            }
            if job.clears_pending_attachments {
                app.clear_sent_pending_attachments();
            }
            app.status = None;
            app.active_run_id = None;
            *active_run = None;
        }
        Ok(Err(error)) => {
            app.status = Some(error.message);
            app.active_run_id = None;
            *active_run = None;
        }
        Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => {
            app.status = Some("Active run worker disconnected.".into());
            app.active_run_id = None;
            *active_run = None;
        }
    }
}

fn load_run_detail(
    globals: &GlobalOptions,
    project_id: &str,
    run_id: &str,
) -> Result<RunDetail, CliError> {
    let value = invoke_json(
        globals,
        &["conversation", "show", "--project-id", project_id, run_id],
    )?;
    let snapshot = value.get("snapshot").cloned().unwrap_or(value);
    Ok(run_detail_from_snapshot(&snapshot))
}

pub fn run_detail_from_snapshot(snapshot: &JsonValue) -> RunDetail {
    let mut messages = snapshot
        .get("messages")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .map(|message| {
            let metadata = message.get("providerMetadata");
            let thinking = metadata
                .and_then(|metadata| metadata.get("reasoningContent"))
                .and_then(JsonValue::as_str)
                .filter(|reasoning| !reasoning.trim().is_empty())
                .map(str::to_owned);
            let tool_calls = metadata
                .and_then(|metadata| metadata.get("assistantToolCalls"))
                .and_then(JsonValue::as_array)
                .into_iter()
                .flatten()
                .map(|tool| {
                    let name = string_field(tool, "providerToolName");
                    let detail = tool
                        .get("arguments")
                        .and_then(|args| super::transcript::summarize_tool_arguments(&name, args));
                    let tool_call_id = tool
                        .get("toolCallId")
                        .and_then(JsonValue::as_str)
                        .filter(|id| !id.is_empty())
                        .map(str::to_owned);
                    ToolCallRow {
                        name,
                        tool_call_id,
                        detail,
                        completed_duration: None,
                    }
                })
                .filter(|tool| !tool.name.is_empty())
                .collect();
            RuntimeMessageRow {
                role: string_field(message, "role"),
                content: string_field(message, "content"),
                attachments: message
                    .get("attachments")
                    .and_then(JsonValue::as_array)
                    .into_iter()
                    .flatten()
                    .map(runtime_attachment_row_from_json)
                    .filter(|attachment| !attachment.original_name.is_empty())
                    .collect(),
                thinking,
                tool_calls,
            }
        })
        .collect::<Vec<_>>();
    let mut tokens_used: Option<u64> = None;
    let mut context_window: Option<u64> = None;
    let mut tool_durations: std::collections::HashMap<String, Duration> =
        std::collections::HashMap::new();
    let mut in_progress_text: Option<String> = None;
    let mut in_progress_reasoning: Option<String> = None;
    let events = snapshot
        .get("events")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .map(|event| {
            let payload = event.get("payload").cloned().unwrap_or(JsonValue::Null);
            // Streamed assistant deltas: take the latest non-empty
            // text/reasoning so the inline preview reflects the most
            // recent state. We let later events overwrite earlier ones
            // since the backend emits cumulative content per delta.
            if string_field(event, "eventKind") == "message_delta"
                && payload.get("inProgress").and_then(JsonValue::as_bool) == Some(true)
                && payload.get("role").and_then(JsonValue::as_str) == Some("assistant")
            {
                if let Some(text) = payload.get("text").and_then(JsonValue::as_str) {
                    if !text.is_empty() {
                        in_progress_text = Some(text.to_owned());
                    }
                }
                if let Some(reasoning) = payload.get("reasoningText").and_then(JsonValue::as_str) {
                    if !reasoning.is_empty() {
                        in_progress_reasoning = Some(reasoning.to_owned());
                    }
                }
            }
            // Pick up token usage from any event that carries it. The
            // headless runtime emits these on `validationCompleted` and
            // `runCompleted`, but be permissive about the exact shape so
            // we don't break when the backend evolves.
            if let Some(value) = u64_field(&payload, "tokensUsed") {
                tokens_used = Some(value);
            }
            if let Some(value) = u64_field(&payload, "contextWindow") {
                context_window = Some(value);
            }
            if let Some(usage) = payload.get("usage") {
                if let Some(value) = u64_field(usage, "totalTokens") {
                    tokens_used = Some(value);
                }
                if let Some(value) = u64_field(usage, "contextWindow") {
                    context_window = Some(value);
                }
            }
            // `tool_completed` events carry per-call timing in
            // `payload.dispatch.elapsedMs` (or, on older shapes,
            // `payload.elapsedMs`). Index them so we can join back to
            // the assistant message's tool calls below.
            if string_field(event, "eventKind") == "tool_completed" {
                if let Some(id) = payload
                    .get("toolCallId")
                    .and_then(JsonValue::as_str)
                    .filter(|id| !id.is_empty())
                {
                    let elapsed_ms = payload
                        .get("dispatch")
                        .and_then(|dispatch| dispatch.get("elapsedMs"))
                        .and_then(JsonValue::as_u64)
                        .or_else(|| u64_field(&payload, "elapsedMs"));
                    if let Some(ms) = elapsed_ms {
                        tool_durations.insert(id.to_owned(), Duration::from_millis(ms));
                    }
                }
            }
            RuntimeEventRow {
                event_kind: string_field(event, "eventKind"),
                summary: payload
                    .get("summary")
                    .or_else(|| payload.get("message"))
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_owned(),
                action_id: payload
                    .get("actionId")
                    .or_else(|| payload.get("action_id"))
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned),
                safe_read_approval: payload
                    .get("annotations")
                    .and_then(|annotations| annotations.get("readOnlyHint"))
                    .and_then(JsonValue::as_bool)
                    .unwrap_or(false),
            }
        })
        .collect::<Vec<_>>();
    // Join per-call durations back onto each tool pill so completed
    // calls render their real elapsed time.
    for message in &mut messages {
        for tool in &mut message.tool_calls {
            if tool.completed_duration.is_none() {
                if let Some(id) = tool.tool_call_id.as_deref() {
                    if let Some(duration) = tool_durations.get(id).copied() {
                        tool.completed_duration = Some(duration);
                    }
                }
            }
        }
    }
    // The backend keeps emitting in-progress deltas, then writes a final
    // assistant message. Once that message is in `messages`, the deltas
    // are stale — drop the preview so we don't double-render.
    if let Some(latest) = messages
        .iter()
        .rev()
        .find(|m| matches!(m.role.as_str(), "assistant" | "model"))
    {
        if let Some(text) = in_progress_text.as_deref() {
            if !latest.content.is_empty() && latest.content.starts_with(text) {
                in_progress_text = None;
            }
        }
        if let Some(reasoning) = in_progress_reasoning.as_deref() {
            if latest
                .thinking
                .as_deref()
                .is_some_and(|t| !t.is_empty() && t.starts_with(reasoning))
            {
                in_progress_reasoning = None;
            }
        }
    }
    RunDetail {
        run_id: string_field(snapshot, "runId"),
        status: string_field(snapshot, "status"),
        messages,
        events,
        in_progress_text,
        in_progress_reasoning,
        tokens_used,
        context_window,
        started_at: None,
    }
}

fn u64_field(value: &JsonValue, key: &str) -> Option<u64> {
    value.get(key).and_then(JsonValue::as_u64)
}

/// Persisted preferences mirroring `xero.agent.composer.settings.v1` from
/// the desktop app, scoped to the TUI. We keep our own JSON file because
/// the Tauri side stores its state in `localStorage` — both writers
/// converge on the same field names so a future bridge is trivial.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct TuiPreferences {
    runtime_agent_id: Option<String>,
    thinking_effort: Option<String>,
    provider_id: Option<String>,
    model_id: Option<String>,
}

impl TuiPreferences {
    fn load(path: &PathBuf) -> Option<Self> {
        let bytes = fs::read(path).ok()?;
        serde_json::from_slice(&bytes).ok()
    }

    fn save(&self, path: &PathBuf) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let payload = serde_json::to_vec_pretty(self)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
        fs::write(path, payload)
    }
}

fn preferences_path_for(state_dir: &std::path::Path) -> PathBuf {
    state_dir.join("tui-preferences.json")
}

/// Location of the remote-bridge identity file used by `xero remote login`.
/// Kept in sync with [`crate::remote_cli`] — both paths must agree so the
/// TUI banner reflects what `xero remote login` writes.
fn remote_identity_path(globals: &GlobalOptions) -> PathBuf {
    crate::cli_app_data_root(globals)
        .join("remote")
        .join("desktop-identity.json")
}

/// Read the signed-in GitHub handle from the identity file. Returns
/// `None` when the file is missing, malformed, or has no `githubLogin`
/// field — all benign states meaning "not signed in" to the user.
fn load_signed_in_handle(identity_path: &Path) -> Option<String> {
    let bytes = fs::read(identity_path).ok()?;
    let identity: JsonValue = serde_json::from_slice(&bytes).ok()?;
    let handle = identity
        .get("githubLogin")
        .and_then(JsonValue::as_str)?
        .trim();
    if handle.is_empty() {
        None
    } else {
        Some(format!("@{handle}"))
    }
}

fn load_agents(
    globals: &GlobalOptions,
    project_id: Option<&str>,
) -> Result<Vec<AgentEntry>, CliError> {
    // Without a registered project the catalog is empty — fall back to the
    // hardcoded seed list so the UI still has something to cycle.
    let Some(project_id) = project_id else {
        return Ok(default_agent_catalog());
    };
    let value = invoke_json(
        globals,
        &["agent-definition", "list", "--project-id", project_id],
    )?;
    let agents = value
        .get("definitions")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter(|definition| {
            // Hide archived/blocked agents so they don't appear in the cycle.
            let lifecycle = string_field(definition, "lifecycleState");
            !matches!(lifecycle.as_str(), "archived" | "blocked")
        })
        .map(|definition| AgentEntry {
            definition_id: string_field(definition, "definitionId"),
            display_name: string_field(definition, "displayName"),
        })
        .filter(|entry| !entry.definition_id.is_empty())
        .collect::<Vec<_>>();
    if agents.is_empty() {
        Ok(default_agent_catalog())
    } else {
        Ok(merge_missing_default_agents(agents))
    }
}

fn load_providers(globals: &GlobalOptions) -> Result<Vec<ProviderRow>, CliError> {
    let value = invoke_json(globals, &["provider", "list"])?;
    Ok(value
        .get("providers")
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .map(|provider| ProviderRow {
            provider_id: string_field(provider, "providerId"),
            default_model: string_field(provider, "defaultModel"),
            credential_kind: string_field(provider, "credentialKind"),
        })
        .collect())
}

pub(crate) fn invoke_json(globals: &GlobalOptions, args: &[&str]) -> Result<JsonValue, CliError> {
    if let Some(adapter) = globals.tui_adapter.as_ref() {
        let owned = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        if let Some(result) = adapter.invoke_json(&globals.state_dir, &owned) {
            return result;
        }
    }
    let mut invocation = vec![
        "xero".to_owned(),
        "--json".to_owned(),
        "--state-dir".to_owned(),
        globals.state_dir.to_string_lossy().into_owned(),
    ];
    invocation.extend(args.iter().map(|arg| (*arg).to_owned()));
    crate::run_with_args(invocation).map(|response| response.json)
}

fn string_field(value: &JsonValue, key: &str) -> String {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn runtime_attachment_row_from_json(value: &JsonValue) -> RuntimeAttachmentRow {
    RuntimeAttachmentRow {
        kind: string_field(value, "kind"),
        original_name: string_field(value, "originalName"),
        size_bytes: value
            .get("sizeBytes")
            .and_then(JsonValue::as_i64)
            .unwrap_or_default(),
    }
}

/// Render the inline viewport: optional streaming indicator on top,
/// then the composer block, a lift-gap row, then the footer. Everything
/// else (welcome banner, prompts, assistant responses, tool pills) lives
/// in the terminal's scrollback and was already emitted via
/// `commit_pending_history`.
pub fn render(frame: &mut Frame<'_>, app: &App) {
    let root = frame.area();
    frame.render_widget(Clear, root);
    let canvas = ratatui::widgets::Paragraph::new("").style(theme::base());
    frame.render_widget(canvas, root);
    render_inline_surface(frame, root, app);
    render_palette(frame, root, app);
}

fn render_fullscreen(frame: &mut Frame<'_>, app: &App) {
    let root = frame.area();
    frame.render_widget(Clear, root);
    let canvas = ratatui::widgets::Paragraph::new("").style(theme::base());
    frame.render_widget(canvas, root);

    let terminal_height = root.height;
    let bottom_height = desired_bottom_panel_height(app, terminal_height).min(root.height);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(bottom_height)])
        .split(root);
    render_fullscreen_history(frame, chunks[0], app);
    render_inline_surface(frame, chunks[1], app);
    render_palette(frame, root, app);
}

fn render_inline_surface(frame: &mut Frame<'_>, root: Rect, app: &App) {
    let footer_height = u16::from(root.height > 0);
    let gap_height = u16::from(root.height > footer_height + 1);
    let composer_natural = composer::height(app);
    // Keep a base-background row between scrollback conversation content
    // and the elevated composer whenever the panel has room for the full
    // composer, composer/footer gap, and footer.
    let conversation_gap_height = CONVERSATION_COMPOSER_GAP_ROWS.min(
        root.height
            .saturating_sub(composer_natural + footer_height + gap_height),
    );
    let composer_height = composer_natural.min(
        root.height
            .saturating_sub(footer_height + gap_height + conversation_gap_height),
    );
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),                          // streaming indicator
            Constraint::Length(conversation_gap_height), // conversation-to-composer gap
            Constraint::Length(composer_height),         // composer
            Constraint::Length(gap_height),              // composer-to-footer gap
            Constraint::Length(footer_height),           // footer
        ])
        .split(root);

    if matches!(
        app.run_detail.as_ref().map(|detail| detail.status.as_str()),
        Some("running")
    ) {
        transcript::render_inline_thinking(frame, chunks[0], app);
    }

    if chunks[2].height > 0 {
        composer::render(frame, chunks[2], app);
    }
    if chunks[4].height > 0 {
        footer::render(frame, chunks[4], app);
    }
}

fn render_palette(frame: &mut Frame<'_>, root: Rect, app: &App) {
    if app.palette.is_some() {
        let palette_area = palette_dialog_area(app, root);
        frame.render_widget(Clear, palette_area);
        palette::render(frame, palette_area, app);
    }
}

fn render_fullscreen_history(frame: &mut Frame<'_>, area: Rect, app: &App) {
    if area.height == 0 {
        return;
    }
    let lines = fullscreen_history_lines(app, area.width, area.height);
    let start = lines.len().saturating_sub(area.height as usize);
    let visible = lines.into_iter().skip(start).collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(visible)
            .style(theme::base())
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn fullscreen_history_lines(app: &App, width: u16, height: u16) -> Vec<Line<'static>> {
    let Some(detail) = app.run_detail.as_ref() else {
        return transcript::welcome_banner_lines(
            width,
            height,
            env!("CARGO_PKG_VERSION"),
            app.signed_in_handle.as_deref(),
        );
    };

    let mut lines = Vec::new();
    for (idx, message) in detail
        .messages
        .iter()
        .filter(|message| !transcript::is_hidden_role(&message.role))
        .enumerate()
    {
        if idx > 0 {
            let blank_rows = if message.role == "user" { 2 } else { 1 };
            for _ in 0..blank_rows {
                lines.push(Line::raw(""));
            }
        }
        let mut message_lines = transcript::message_lines(message, width);
        let pills = transcript::tool_pill_lines(&message.tool_calls);
        if !pills.is_empty()
            && !message_lines.is_empty()
            && !is_blank_line(message_lines.last().unwrap())
        {
            message_lines.push(Line::raw(""));
        }
        lines.extend(message_lines);
        lines.extend(pills);
    }

    if detail.status == "running" {
        if let Some(reasoning) = detail
            .in_progress_reasoning
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            lines.push(Line::raw(""));
            lines.extend(completed_stream_rows(
                transcript::assistant_thinking_lines(reasoning, width),
                reasoning,
                width,
            ));
        }
        if let Some(text) = detail
            .in_progress_text
            .as_deref()
            .filter(|text| !text.trim().is_empty())
        {
            lines.push(Line::raw(""));
            lines.extend(completed_stream_rows(
                transcript::assistant_markdown_wrapped_lines(text, width),
                text,
                width,
            ));
        }
    }

    if lines.is_empty() {
        lines.push(Line::raw(""));
    }
    lines
}

fn palette_dialog_area(app: &App, root: Rect) -> Rect {
    let available = root;
    let (width, height) = palette::desired_size(app, available);
    let width = width.min(available.width);
    let height = height.min(available.height);
    Rect {
        x: available.x + available.width.saturating_sub(width) / 2,
        y: available.y + available.height.saturating_sub(height) / 2,
        width,
        height,
    }
}

/// Push any not-yet-committed history rows to the terminal's scrollback
/// via `Terminal::insert_before`. This is what gives the TUI native
/// scrolling: the terminal owns the history, so its mouse wheel and
/// scrollbar work like in any other CLI program.
fn commit_pending_history<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), CliError> {
    let size = terminal.size().map_err(tui_io_error)?;
    let width = size.width;

    // Welcome banner — emitted once, before the first prompt. We pad it
    // out to (terminal_height - inline_viewport_height) so the very first
    // paint fills the whole screen instead of dropping a 6-row sliver
    // into the cursor's current position.
    if !app.welcome_committed && app.run_detail.is_none() {
        let banner_height = size
            .height
            .saturating_sub(app.inline_height)
            .max(transcript::welcome_logo_height());
        let lines = transcript::welcome_banner_lines(
            width,
            banner_height,
            env!("CARGO_PKG_VERSION"),
            app.signed_in_handle.as_deref(),
        );
        emit_to_scrollback(terminal, app, lines, width)?;
        app.welcome_committed = true;
        app.committed_scrollback.push(ScrollbackEntry::Welcome);
    }

    // New transcript messages.
    if let Some(detail) = app.run_detail.as_ref() {
        let run_running = detail.status == "running";
        // The committed counter is reset on each new run (start_prompt_job).
        let visible: Vec<_> = detail
            .messages
            .iter()
            .filter(|message| !transcript::is_hidden_role(&message.role))
            .cloned()
            .collect();
        while app.committed_messages < visible.len() {
            let idx = app.committed_messages;
            let message = &visible[idx];
            // Hold any message with pending tool calls until those calls
            // resolve. Otherwise the pill renders with no duration. Once
            // the run reaches a terminal state we flush everything so
            // nothing gets stuck.
            let pending = message
                .tool_calls
                .iter()
                .any(|tool| tool.completed_duration.is_none());
            if run_running && pending {
                break;
            }
            // For assistant messages, build the lines from thinking +
            // content separately so we can skip any prefix that's
            // already been streamed into scrollback. Other roles still
            // go through the normal `message_lines` path.
            let assistant_already_streamed = matches!(message.role.as_str(), "assistant" | "model")
                && (app.streamed_text_rows > 0 || app.streamed_reasoning_rows > 0);
            let mut lines = if matches!(message.role.as_str(), "assistant" | "model") {
                let mut acc: Vec<ratatui::text::Line<'static>> = Vec::new();
                if let Some(thinking) = message.thinking.as_deref() {
                    let thinking_lines = transcript::assistant_thinking_lines(thinking, width);
                    let start = app.streamed_reasoning_rows.min(thinking_lines.len());
                    acc.extend(thinking_lines.into_iter().skip(start));
                    if !acc.is_empty() && !message.content.is_empty() {
                        acc.push(ratatui::text::Line::raw(""));
                    }
                }
                if !message.content.is_empty() {
                    if app.streamed_text_rows > 0 {
                        let content_lines =
                            transcript::assistant_markdown_wrapped_lines(&message.content, width);
                        let start = app.streamed_text_rows.min(content_lines.len());
                        if start < content_lines.len() {
                            if !acc.is_empty() && !is_blank_line(acc.last().unwrap()) {
                                acc.push(ratatui::text::Line::raw(""));
                            }
                            acc.extend(content_lines.into_iter().skip(start));
                        }
                    } else {
                        if (!acc.is_empty() && !is_blank_line(acc.last().unwrap()))
                            || (acc.is_empty() && app.streamed_reasoning_rows > 0)
                        {
                            acc.push(ratatui::text::Line::raw(""));
                        }
                        acc.extend(transcript::assistant_markdown_lines(&message.content));
                    }
                }
                acc
            } else {
                transcript::message_lines(message, width)
            };
            if idx > 0 && !assistant_already_streamed {
                // Single separator blank between messages. Follow-up
                // user prompts get an extra row because the user
                // prompt block has its own elevated bg-pad rows that
                // visually blend with a single black blank.
                let blank_rows = if message.role == "user" { 2 } else { 1 };
                for _ in 0..blank_rows {
                    lines.insert(0, ratatui::text::Line::raw(""));
                }
            }
            // Pills slot in after thinking/content. They no longer
            // carry their own leading blank, so we insert one here
            // when the prior content doesn't already end on a blank.
            let pills = transcript::tool_pill_lines(&message.tool_calls);
            let needs_pill_separator = !pills.is_empty()
                && ((!lines.is_empty() && !is_blank_line(lines.last().unwrap()))
                    || (lines.is_empty()
                        && assistant_already_streamed
                        && !app.last_scrollback_line_blank));
            if needs_pill_separator {
                lines.push(ratatui::text::Line::raw(""));
            }
            lines.extend(pills);
            emit_to_scrollback(terminal, app, lines, width)?;
            app.committed_scrollback
                .push(ScrollbackEntry::Message(message.clone()));
            app.committed_messages += 1;
            if matches!(message.role.as_str(), "assistant" | "model") {
                app.streamed_text_rows = 0;
                app.streamed_reasoning_rows = 0;
            }
        }
    }

    // Stream only completed rows of the in-progress assistant message
    // into scrollback. The currently-forming row is intentionally
    // hidden so token-by-token provider deltas never show up as flicker.
    // Reasoning streams first (the model thinks before it speaks).
    commit_streaming_reasoning_rows(terminal, app, width)?;
    commit_streaming_rows(terminal, app, width)?;
    Ok(())
}

fn replay_committed_scrollback<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> Result<(), CliError> {
    let entries = app.committed_scrollback.clone();
    app.last_scrollback_line_blank = false;
    if entries.is_empty() {
        return Ok(());
    }

    let size = terminal.size().map_err(tui_io_error)?;
    let width = size.width;
    let mut committed_messages = 0usize;
    for entry in entries {
        let lines = match entry {
            ScrollbackEntry::Welcome => {
                let banner_height = size
                    .height
                    .saturating_sub(app.inline_height)
                    .max(transcript::welcome_logo_height());
                transcript::welcome_banner_lines(
                    width,
                    banner_height,
                    env!("CARGO_PKG_VERSION"),
                    app.signed_in_handle.as_deref(),
                )
            }
            ScrollbackEntry::Message(message) => {
                let lines = replay_message_lines(&message, width, committed_messages > 0);
                committed_messages += 1;
                lines
            }
        };
        emit_to_scrollback(terminal, app, lines, width)?;
    }
    Ok(())
}

fn replay_message_lines(
    message: &RuntimeMessageRow,
    width: u16,
    has_previous_message: bool,
) -> Vec<ratatui::text::Line<'static>> {
    let mut lines = transcript::message_lines(message, width);
    if has_previous_message {
        let blank_rows = if message.role == "user" { 2 } else { 1 };
        for _ in 0..blank_rows {
            lines.insert(0, ratatui::text::Line::raw(""));
        }
    }
    let pills = transcript::tool_pill_lines(&message.tool_calls);
    if !pills.is_empty() && !lines.is_empty() && !is_blank_line(lines.last().unwrap()) {
        lines.push(ratatui::text::Line::raw(""));
    }
    lines.extend(pills);
    lines
}

fn commit_streaming_reasoning_rows<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    width: u16,
) -> Result<(), CliError> {
    let Some(detail) = app.run_detail.as_ref() else {
        return Ok(());
    };
    if detail.status != "running" {
        return Ok(());
    }
    let Some(text) = detail.in_progress_reasoning.as_deref() else {
        return Ok(());
    };
    if text.trim().is_empty() {
        return Ok(());
    }
    let completed = completed_stream_rows(
        transcript::assistant_thinking_lines(text, width),
        text,
        width,
    );
    if app.streamed_reasoning_rows > completed.len() {
        app.streamed_reasoning_rows = 0;
    }
    if completed.len() <= app.streamed_reasoning_rows {
        return Ok(());
    }
    let previous_rows = app.streamed_reasoning_rows;
    let emitted_rows = completed.len() - previous_rows;
    let mut lines: Vec<_> = completed.into_iter().skip(previous_rows).collect();
    if previous_rows == 0 {
        lines.insert(0, ratatui::text::Line::raw(""));
    }
    emit_to_scrollback(terminal, app, lines, width)?;
    app.streamed_reasoning_rows += emitted_rows;
    Ok(())
}

fn commit_streaming_rows<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    width: u16,
) -> Result<(), CliError> {
    let Some(detail) = app.run_detail.as_ref() else {
        return Ok(());
    };
    if detail.status != "running" {
        return Ok(());
    }
    let Some(text) = detail.in_progress_text.as_deref() else {
        return Ok(());
    };
    let completed = completed_stream_rows(
        transcript::assistant_markdown_wrapped_lines(text, width),
        text,
        width,
    );
    if app.streamed_text_rows > completed.len() {
        app.streamed_text_rows = 0;
    }
    if completed.len() <= app.streamed_text_rows {
        return Ok(());
    }
    let previous_rows = app.streamed_text_rows;
    let emitted_rows = completed.len() - previous_rows;
    let mut lines: Vec<_> = completed.into_iter().skip(previous_rows).collect();
    if previous_rows == 0 {
        lines.insert(0, ratatui::text::Line::raw(""));
    }
    emit_to_scrollback(terminal, app, lines, width)?;
    app.streamed_text_rows += emitted_rows;
    Ok(())
}

fn completed_stream_rows(
    mut lines: Vec<ratatui::text::Line<'static>>,
    source: &str,
    width: u16,
) -> Vec<ratatui::text::Line<'static>> {
    if lines.is_empty() {
        return lines;
    }
    if source_ends_at_row_boundary(source)
        || lines
            .last()
            .is_some_and(|line| line_visible_width(line) >= width as usize)
    {
        return lines;
    }
    lines.pop();
    lines
}

fn source_ends_at_row_boundary(source: &str) -> bool {
    source.ends_with('\n') || source.ends_with('\r')
}

fn line_visible_width(line: &ratatui::text::Line<'_>) -> usize {
    line.spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum()
}

/// Single entry point for scrollback writes. Drops a leading blank from
/// `lines` if the previous emit ended on a blank, so independent emit
/// calls (welcome, message, tool block, streaming chunk) compose into
/// single-spaced paragraphs instead of accumulating double-blank seams.
fn emit_to_scrollback<B: Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
    mut lines: Vec<ratatui::text::Line<'static>>,
    width: u16,
) -> Result<(), CliError> {
    if lines.is_empty() {
        return Ok(());
    }
    // Drop at most one leading blank when the previous emit also ended
    // on a blank — collapses seams without erasing intentional double
    // padding (e.g. follow-up user prompts request two leading blanks).
    if app.last_scrollback_line_blank && lines.first().map(is_blank_line).unwrap_or(false) {
        lines.remove(0);
    }
    if lines.is_empty() {
        return Ok(());
    }
    let trailing_blank = lines.last().map(is_blank_line).unwrap_or(false);
    write_scrollback(terminal, lines, width)?;
    app.last_scrollback_line_blank = trailing_blank;
    Ok(())
}

fn is_blank_line(line: &ratatui::text::Line<'_>) -> bool {
    line.spans.iter().all(|span| span.content.trim().is_empty())
}

fn write_scrollback(
    terminal: &mut Terminal<impl Backend>,
    lines: Vec<ratatui::text::Line<'static>>,
    width: u16,
) -> Result<(), CliError> {
    if lines.is_empty() {
        return Ok(());
    }
    let height = lines.len() as u16;
    terminal
        .insert_before(height, |buf: &mut Buffer| {
            let area = Rect {
                x: 0,
                y: 0,
                width,
                height,
            };
            Paragraph::new(lines)
                .style(theme::base())
                .wrap(Wrap { trim: false })
                .render(area, buf);
        })
        .map_err(tui_io_error)?;
    Ok(())
}

pub fn smoke_snapshot(globals: &GlobalOptions) -> Result<JsonValue, CliError> {
    let project = super::project::resolve(globals);
    let mut app = App::new(globals, project);
    let mut terminal = Terminal::with_options(
        ratatui::backend::TestBackend::new(100, 32),
        TerminalOptions {
            viewport: Viewport::Inline(app.inline_height),
        },
    )
    .map_err(|error| {
        CliError::system_fault(
            "xero_tui_smoke_terminal_failed",
            format!("Could not create TUI test backend: {error}"),
        )
    })?;
    commit_pending_history(&mut terminal, &mut app)?;
    terminal
        .draw(|frame| render(frame, &app))
        .map_err(|error| {
            CliError::system_fault(
                "xero_tui_smoke_render_failed",
                format!("Could not render TUI smoke frame: {error}"),
            )
        })?;
    let backend = terminal.backend();
    let buffer_text = backend
        .scrollback()
        .content
        .iter()
        .chain(backend.buffer().content.iter())
        .map(|cell| cell.symbol())
        .collect::<String>();
    // The ASCII wordmark uses full-block glyphs rather than the literal
    // text "xero" — the smoke needs to assert *something* from the logo
    // surface still renders.
    if !buffer_text.contains('\u{2588}') {
        return Err(CliError::system_fault(
            "xero_tui_smoke_missing_logo",
            "Rendered TUI frame did not contain the xero wordmark.",
        ));
    }
    Ok(json!({
        "kind": "tuiSmoke",
        "projectId": app.project.project_id,
        "registered": app.project.registered,
        "providers": app.providers.len(),
    }))
}

pub fn smoke_fake_provider_run(globals: &GlobalOptions) -> Result<JsonValue, CliError> {
    let project = super::project::resolve(globals);
    let mut app = App::new(globals, project);
    app.fake_provider_fixture = true;

    let provider_id = app
        .selected_provider_id()
        .map(str::to_owned)
        .unwrap_or_else(|| "fake_provider".into());
    let model_id = app
        .selected_model_id()
        .map(str::to_owned)
        .unwrap_or_else(|| "fake-model".into());
    let runtime_agent_id = app
        .selected_agent_definition_id()
        .unwrap_or("engineer")
        .to_owned();
    let thinking_effort = app.thinking_effort.label().to_owned();
    let run_id = generate_id("tui-run");
    let project_id = app.project.project_id.clone();
    let mut owned = vec![
        "agent".to_owned(),
        "exec".to_owned(),
        "--provider".to_owned(),
        provider_id,
        "--model".to_owned(),
        model_id,
        "--run-id".to_owned(),
        run_id.clone(),
        "--runtime-agent-id".to_owned(),
        runtime_agent_id.clone(),
        "--agent-definition-id".to_owned(),
        runtime_agent_id,
        "--thinking-effort".to_owned(),
        thinking_effort,
        "--prompt".to_owned(),
        "TUI fake-provider smoke: respond with a compact status line.".to_owned(),
    ];
    if let Some(project_id) = project_id.as_deref() {
        owned.push("--project-id".into());
        owned.push(project_id.to_owned());
    }
    let borrowed = owned.iter().map(String::as_str).collect::<Vec<_>>();
    let value = invoke_json(globals, &borrowed)?;
    let snapshot = value.get("snapshot").cloned().unwrap_or(value);
    let detail = run_detail_from_snapshot(&snapshot);
    app.run_detail = Some(detail.clone());
    app.status = None;

    let mut terminal =
        Terminal::new(ratatui::backend::TestBackend::new(100, 32)).map_err(|error| {
            CliError::system_fault(
                "xero_tui_smoke_run_terminal_failed",
                format!("Could not create TUI test backend: {error}"),
            )
        })?;
    terminal
        .draw(|frame| render(frame, &app))
        .map_err(|error| {
            CliError::system_fault(
                "xero_tui_smoke_run_render_failed",
                format!("Could not render TUI fake-provider run smoke frame: {error}"),
            )
        })?;
    Ok(json!({
        "kind": "tuiSmokeRun",
        "runId": run_id,
        "projectId": project_id,
        "messages": detail.messages.len(),
        "events": detail.events.len(),
    }))
}

fn tui_io_error(error: io::Error) -> CliError {
    CliError::system_fault("xero_tui_io_failed", format!("Terminal UI failed: {error}"))
}

#[cfg(test)]
pub(crate) fn test_only_empty_app() -> App {
    let project = super::project::ResolvedProject {
        project_id: None,
        root: std::path::PathBuf::from("/tmp/xero-test"),
        branch: Some("main".into()),
        display_path: "~/xero-test".into(),
        registered: false,
    };
    App {
        project,
        providers: Vec::new(),
        selected_provider: 0,
        fake_provider_fixture: false,
        agents: default_agent_catalog(),
        selected_agent: 0,
        thinking_effort: ThinkingEffort::High,
        composer: String::new(),
        composer_cursor: 0,
        composer_desired_column: None,
        slash_selected: 0,
        palette: None,
        status: None,
        update_notice: None,
        update_check_receiver: None,
        run_detail: None,
        active_run_id: None,
        active_session_id: None,
        draft_run_id: None,
        pending_attachments: Vec::new(),
        next_attachment_index: 1,
        last_ctrl_c_at: None,
        committed_messages: 0,
        welcome_committed: false,
        tui_reset_requested: false,
        streamed_text_rows: 0,
        streamed_reasoning_rows: 0,
        inline_height: INLINE_HEIGHT_IDLE,
        last_scrollback_line_blank: false,
        committed_scrollback: Vec::new(),
        preferences_path: PathBuf::from("/tmp/xero-test-prefs.json"),
        identity_path: PathBuf::from("/tmp/xero-test-identity.json"),
        signed_in_handle: None,
        login_flow_id: None,
    }
}

#[cfg(test)]
pub(crate) fn test_only_globals() -> GlobalOptions {
    crate::GlobalOptions {
        state_dir: std::path::PathBuf::from("/tmp/xero-test-state"),
        output_mode: crate::OutputMode::Text,
        ci: false,
        tui_adapter: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingAdapter {
        calls: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl crate::TuiCommandAdapter for RecordingAdapter {
        fn invoke_json(
            &self,
            _state_dir: &Path,
            args: &[String],
        ) -> Option<Result<JsonValue, CliError>> {
            self.calls.lock().expect("record calls").push(args.to_vec());
            Some(Ok(json!({
                "snapshot": {
                    "runId": "run-recorded",
                    "status": "completed",
                    "messages": [],
                    "events": []
                }
            })))
        }
    }

    #[derive(Default)]
    struct NewSessionAdapter {
        calls: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl crate::TuiCommandAdapter for NewSessionAdapter {
        fn invoke_json(
            &self,
            _state_dir: &Path,
            args: &[String],
        ) -> Option<Result<JsonValue, CliError>> {
            self.calls.lock().expect("record calls").push(args.to_vec());
            Some(Ok(json!({
                "session": {
                    "agentSessionId": "session-new"
                }
            })))
        }
    }

    #[derive(Default)]
    struct ProviderListAdapter;

    impl crate::TuiCommandAdapter for ProviderListAdapter {
        fn invoke_json(
            &self,
            _state_dir: &Path,
            args: &[String],
        ) -> Option<Result<JsonValue, CliError>> {
            match args {
                [command, subcommand] if command == "provider" && subcommand == "list" => {
                    Some(Ok(json!({
                        "kind": "providerList",
                        "providers": [
                            {
                                "providerId": "openai_codex",
                                "label": "OpenAI Codex",
                                "defaultModel": "gpt-5.2",
                                "credentialKind": "app_session",
                                "headlessStatus": "configured",
                                "models": [
                                    {
                                        "modelId": "gpt-5.2",
                                        "displayName": "GPT-5.2",
                                        "thinkingSupported": true
                                    },
                                    {
                                        "modelId": "gpt-5.3",
                                        "displayName": "GPT-5.3 Codex",
                                        "thinkingSupported": true
                                    }
                                ]
                            }
                        ]
                    })))
                }
                _ => None,
            }
        }
    }

    #[derive(Default)]
    struct AttachmentStageAdapter {
        calls: Arc<Mutex<Vec<Vec<String>>>>,
    }

    impl crate::TuiCommandAdapter for AttachmentStageAdapter {
        fn invoke_json(
            &self,
            _state_dir: &Path,
            args: &[String],
        ) -> Option<Result<JsonValue, CliError>> {
            self.calls.lock().expect("record calls").push(args.to_vec());
            match args.first().map(String::as_str) {
                Some("attachment") if args.get(1).map(String::as_str) == Some("stage") => {
                    let source_path = option_arg(args, "--path").unwrap_or_default();
                    let original_name = Path::new(&source_path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("attachment")
                        .to_owned();
                    Some(Ok(json!({
                        "attachment": {
                            "kind": "image",
                            "absolutePath": "/tmp/xero/staged/attachment.png",
                            "mediaType": "image/png",
                            "originalName": original_name,
                            "sizeBytes": 42,
                            "width": null,
                            "height": null
                        }
                    })))
                }
                Some("agent") if args.get(1).map(String::as_str) == Some("exec") => {
                    Some(Ok(json!({
                        "snapshot": {
                            "runId": "run-recorded",
                            "status": "completed",
                            "messages": [],
                            "events": []
                        }
                    })))
                }
                _ => None,
            }
        }
    }

    fn option_arg(args: &[String], option: &str) -> Option<String> {
        args.windows(2)
            .find(|window| window.first().is_some_and(|arg| arg == option))
            .and_then(|window| window.get(1))
            .cloned()
    }

    fn buffer_to_string(buffer: &Buffer) -> String {
        buffer
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    fn buffer_rows(buffer: &Buffer) -> Vec<String> {
        buffer
            .content
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    fn render_to_string(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|frame| render(frame, app)).expect("draw");
        buffer_to_string(terminal.backend().buffer())
    }

    fn render_rows(app: &App, width: u16, height: u16) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|frame| render(frame, app)).expect("draw");
        buffer_rows(terminal.backend().buffer())
    }

    fn temp_attachment_path(name: &str) -> PathBuf {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock after unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("xero-{nonce}"));
        std::fs::create_dir_all(&dir).expect("create temp attachment dir");
        let path = dir.join(name);
        std::fs::write(&path, b"attachment").expect("write temp attachment");
        path
    }

    fn shell_escape_path(path: &Path) -> String {
        path.to_string_lossy().replace(' ', "\\ ")
    }

    fn rendered_cursor_cell(app: &App, width: u16, height: u16) -> (usize, String) {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|frame| render(frame, app)).expect("draw");
        let buffer = terminal.backend().buffer();
        buffer
            .content
            .chunks(buffer.area.width as usize)
            .enumerate()
            .find_map(|(row_index, row)| {
                row.iter()
                    .find(|cell| cell.fg == theme::composer_bg_color() && cell.bg == theme::FG)
                    .map(|cell| (row_index, cell.symbol().to_owned()))
            })
            .expect("rendered cursor cell")
    }

    fn highlighted_text(app: &App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|frame| render(frame, app)).expect("draw");
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .filter(|cell| cell.fg == theme::composer_bg_color() && cell.bg == theme::FG)
            .map(|cell| cell.symbol())
            .collect::<String>()
    }

    fn terminal_history_to_string(terminal: &Terminal<TestBackend>) -> String {
        format!(
            "{}{}",
            buffer_to_string(terminal.backend().scrollback()),
            buffer_to_string(terminal.backend().buffer())
        )
    }

    fn terminal_history_rows(terminal: &Terminal<TestBackend>) -> Vec<String> {
        let mut rows = buffer_rows(terminal.backend().scrollback());
        rows.extend(buffer_rows(terminal.backend().buffer()));
        rows
    }

    fn has_arg_pair(args: &[String], flag: &str, value: &str) -> bool {
        args.windows(2)
            .any(|pair| pair[0] == flag && pair[1] == value)
    }

    fn enter_key() -> KeyEvent {
        KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_char(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)
    }

    fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    fn empty_app() -> App {
        let project = super::super::project::ResolvedProject {
            project_id: None,
            root: std::path::PathBuf::from("/tmp/xero-test"),
            branch: Some("main".into()),
            display_path: "~/xero-test".into(),
            registered: false,
        };
        App {
            project,
            providers: Vec::new(),
            selected_provider: 0,
            fake_provider_fixture: false,
            agents: default_agent_catalog(),
            selected_agent: 0,
            thinking_effort: ThinkingEffort::High,
            composer: String::new(),
            composer_cursor: 0,
            composer_desired_column: None,
            slash_selected: 0,
            palette: None,
            status: None,
            update_notice: None,
            update_check_receiver: None,
            run_detail: None,
            active_run_id: None,
            active_session_id: None,
            draft_run_id: None,
            pending_attachments: Vec::new(),
            next_attachment_index: 1,
            last_ctrl_c_at: None,
            committed_messages: 0,
            welcome_committed: false,
            tui_reset_requested: false,
            streamed_text_rows: 0,
            streamed_reasoning_rows: 0,
            inline_height: INLINE_HEIGHT_IDLE,
            last_scrollback_line_blank: false,
            committed_scrollback: Vec::new(),
            preferences_path: PathBuf::from("/tmp/xero-test-prefs.json"),
            identity_path: PathBuf::from("/tmp/xero-test-identity.json"),
            signed_in_handle: None,
            login_flow_id: None,
        }
    }

    #[test]
    fn composer_option_backspace_deletes_previous_word() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.replace_composer("run tests now");
        let mut active_run = None;

        let outcome = dispatch_key(
            &mut app,
            modified_key(KeyCode::Backspace, KeyModifiers::ALT),
            &mut active_run,
            &globals,
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer, "run tests ");
    }

    #[test]
    fn composer_command_backspace_clears_current_line() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.replace_composer("first line\nsecond line");
        let mut active_run = None;

        let outcome = dispatch_key(
            &mut app,
            modified_key(KeyCode::Backspace, KeyModifiers::SUPER),
            &mut active_run,
            &globals,
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer, "first line\n");
    }

    #[test]
    fn composer_control_fallbacks_match_terminal_text_editing() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.replace_composer("run tests now");
        let mut active_run = None;

        let outcome = dispatch_key(
            &mut app,
            modified_key(KeyCode::Char('w'), KeyModifiers::CONTROL),
            &mut active_run,
            &globals,
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer, "run tests ");

        let outcome = dispatch_key(
            &mut app,
            modified_key(KeyCode::Char('u'), KeyModifiers::CONTROL),
            &mut active_run,
            &globals,
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(app.composer.is_empty());
    }

    #[test]
    fn composer_left_arrow_moves_cursor_for_mid_input_insert() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.replace_composer("helo");
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, key(KeyCode::Left), &mut active_run, &globals);
        assert!(matches!(outcome, KeyOutcome::Continue));

        let outcome = dispatch_key(&mut app, key_char('l'), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer, "hello");
        assert_eq!(app.composer_cursor, "hell".len());
        let text = render_to_string(&app, 100, desired_inline_height(&app, 40));
        let (_, cursor_symbol) = rendered_cursor_cell(&app, 100, desired_inline_height(&app, 40));
        assert!(
            text.contains("hello"),
            "composer should render text without injecting cursor glyphs"
        );
        assert_eq!(
            cursor_symbol, "o",
            "composer should highlight the character under the cursor"
        );
    }

    #[test]
    fn composer_option_arrows_move_cursor_by_word() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.replace_composer("run tests now");
        let mut active_run = None;

        let outcome = dispatch_key(
            &mut app,
            modified_key(KeyCode::Left, KeyModifiers::ALT),
            &mut active_run,
            &globals,
        );
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer_cursor, "run tests ".len());

        let outcome = dispatch_key(
            &mut app,
            modified_key(KeyCode::Right, KeyModifiers::ALT),
            &mut active_run,
            &globals,
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer_cursor, "run tests now".len());
    }

    #[test]
    fn composer_command_arrows_move_cursor_to_line_boundaries() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.replace_composer("first line\nsecond line");
        app.composer_cursor = "first line\nsecond".len();
        let mut active_run = None;

        let outcome = dispatch_key(
            &mut app,
            modified_key(KeyCode::Left, KeyModifiers::SUPER),
            &mut active_run,
            &globals,
        );
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer_cursor, "first line\n".len());

        let outcome = dispatch_key(
            &mut app,
            modified_key(KeyCode::Right, KeyModifiers::META),
            &mut active_run,
            &globals,
        );

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer_cursor, "first line\nsecond line".len());
    }

    #[test]
    fn composer_up_down_arrows_move_between_lines() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.replace_composer("abcd\nef\nghij");
        app.composer_cursor = "abcd\nef".len();
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, key(KeyCode::Up), &mut active_run, &globals);
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer_cursor, "ab".len());

        let outcome = dispatch_key(&mut app, key(KeyCode::Down), &mut active_run, &globals);
        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer_cursor, "abcd\nef".len());
    }

    #[test]
    fn inline_viewport_renders_composer_and_footer() {
        // The viewport is the bottom strip ratatui owns; the transcript
        // and welcome logo now live in the terminal's scrollback (emitted
        // via insert_before), not in this buffer.
        let app = empty_app();
        let text = render_to_string(&app, 100, desired_inline_height(&app, 40));
        assert!(
            text.contains(super::super::theme::STRIPE_GLYPH),
            "missing gold stripe glyph"
        );
        assert!(text.contains("ctrl+p /commands"), "missing palette hint");
        assert!(
            text.contains("tab agents"),
            "missing welcome-state tab-agents hint"
        );
        assert!(text.contains("~/xero-test"), "missing cwd in footer");
        assert!(text.contains(":main"), "missing branch in footer");
        assert!(
            text.contains("Ask") || text.contains("Engineer"),
            "missing dynamic agent label in agent line"
        );
        assert!(
            text.contains("think:high"),
            "missing default thinking-effort indicator"
        );
    }

    #[test]
    fn slash_dialog_command_opens_settings_palette() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.replace_composer("/settings");
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, enter_key(), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        match app.palette.as_ref().expect("settings palette") {
            PaletteState::Detail(detail) => {
                assert_eq!(detail.command_id, "settings");
                assert!(matches!(detail.data, palette::DetailData::Rows(_)));
            }
            PaletteState::Browse(_) => panic!("expected settings detail"),
        }
    }

    #[test]
    fn slash_model_opens_current_provider_model_picker() {
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(ProviderListAdapter));
        let mut app = empty_app();
        app.providers = vec![ProviderRow {
            provider_id: "openai_codex".into(),
            default_model: "gpt-5.2".into(),
            credential_kind: "app_session".into(),
        }];
        app.replace_composer("/model");
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, enter_key(), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        match app.palette.as_ref().expect("model palette") {
            PaletteState::Detail(detail) => {
                assert_eq!(detail.command_id, "model");
                let palette::DetailData::Rows(rows) = &detail.data else {
                    panic!("expected model rows");
                };
                assert_eq!(rows[0].title, "GPT-5.2");
                assert_eq!(rows[1].title, "GPT-5.3 Codex");
            }
            PaletteState::Browse(_) => panic!("expected model detail"),
        }
    }

    #[test]
    fn model_palette_selection_updates_active_model_without_changing_provider() {
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(ProviderListAdapter));
        let mut app = empty_app();
        app.providers = vec![ProviderRow {
            provider_id: "openai_codex".into(),
            default_model: "gpt-5.2".into(),
            credential_kind: "app_session".into(),
        }];
        app.replace_composer("/model");
        let mut active_run = None;

        dispatch_key(&mut app, enter_key(), &mut active_run, &globals);
        dispatch_key(&mut app, key(KeyCode::Down), &mut active_run, &globals);
        let outcome = dispatch_key(&mut app, enter_key(), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(app.palette.is_none());
        assert_eq!(app.selected_provider_id(), Some("openai_codex"));
        assert_eq!(app.selected_model_id(), Some("gpt-5.3"));
        assert_eq!(
            app.status.as_deref(),
            Some("Active model: GPT-5.3 Codex (OpenAI Codex)")
        );
    }

    #[test]
    fn slash_cli_command_invokes_adapter_with_project_scope() {
        let adapter = RecordingAdapter::default();
        let calls = Arc::clone(&adapter.calls);
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let mut app = empty_app();
        app.project.project_id = Some("project-1".into());
        app.replace_composer("/session list");
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, enter_key(), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        let calls = calls.lock().expect("recorded calls");
        let args = calls.last().expect("slash command call");
        assert_eq!(
            args,
            &vec![
                "session".to_owned(),
                "list".to_owned(),
                "--project-id".to_owned(),
                "project-1".to_owned()
            ]
        );
        assert!(matches!(app.palette, Some(PaletteState::Detail(_))));
    }

    #[test]
    fn slash_new_resets_tui_for_fresh_welcome_screen() {
        let adapter = NewSessionAdapter::default();
        let calls = Arc::clone(&adapter.calls);
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let mut app = empty_app();
        app.project.project_id = Some("project-1".into());
        app.replace_composer("/new");
        app.welcome_committed = true;
        app.committed_messages = 2;
        app.streamed_text_rows = 3;
        app.streamed_reasoning_rows = 4;
        app.last_scrollback_line_blank = true;
        app.run_detail = Some(RunDetail {
            run_id: "old-run".into(),
            status: "completed".into(),
            messages: vec![RuntimeMessageRow {
                role: "user".into(),
                content: "old prompt".into(),
                attachments: Vec::new(),
                thinking: None,
                tool_calls: Vec::new(),
            }],
            events: Vec::new(),
            in_progress_text: None,
            in_progress_reasoning: None,
            tokens_used: None,
            context_window: None,
            started_at: None,
        });
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, enter_key(), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.active_session_id.as_deref(), Some("session-new"));
        assert!(app.run_detail.is_none());
        assert_eq!(app.committed_messages, 0);
        assert!(!app.welcome_committed);
        assert_eq!(app.streamed_text_rows, 0);
        assert_eq!(app.streamed_reasoning_rows, 0);
        assert!(!app.last_scrollback_line_blank);
        assert!(app.tui_reset_requested);
        assert_eq!(app.status.as_deref(), Some("New session: session-new"));
        let calls = calls.lock().expect("recorded calls");
        assert_eq!(
            calls.last().expect("new session call"),
            &vec![
                "session".to_owned(),
                "create".to_owned(),
                "--project-id".to_owned(),
                "project-1".to_owned(),
                "--title".to_owned(),
                "New Chat".to_owned(),
                "--session-kind".to_owned(),
                "standard".to_owned()
            ]
        );
    }

    #[test]
    fn slash_parser_keeps_quoted_arguments_together() {
        let words =
            parse_slash_words("conversation search \"needle in haystack\" --include-events")
                .expect("parse slash words");
        assert_eq!(
            words,
            vec![
                "conversation".to_owned(),
                "search".to_owned(),
                "needle in haystack".to_owned(),
                "--include-events".to_owned(),
            ]
        );
    }

    #[test]
    fn slash_input_renders_inline_select_and_filters_results() {
        let mut app = empty_app();
        app.replace_composer("/prov");

        let text = render_to_string(&app, 100, INLINE_HEIGHT_SLASH);

        assert_eq!(desired_inline_height(&app, 40), INLINE_HEIGHT_SLASH);
        assert!(
            text.contains("/providers"),
            "slash select should show matching commands"
        );
        assert!(
            !text.contains("/sessions"),
            "slash select should filter non-matching commands"
        );
        assert!(app.palette.is_none(), "slash select must not open palette");
    }

    #[test]
    fn slash_root_suggestions_keep_steady_inline_viewport_height() {
        let mut app = empty_app();
        let idle_height = desired_inline_height(&app, 40);
        app.replace_composer("/");

        assert!(slash::is_visible(&app));
        assert_eq!(
            desired_inline_height(&app, 40),
            idle_height,
            "opening slash suggestions should not request an inline viewport rebuild"
        );
    }

    #[test]
    fn slash_partial_enter_uses_selected_inline_suggestion() {
        let mut app = empty_app();
        app.replace_composer("/prov");

        let selected = slash::selected_submission(&app, "/prov");

        assert_eq!(selected.as_deref(), Some("/providers"));
    }

    #[test]
    fn slash_model_partial_enter_uses_model_picker() {
        let mut app = empty_app();
        app.replace_composer("/mod");

        let selected = slash::selected_submission(&app, "/mod");

        assert_eq!(selected.as_deref(), Some("/model"));
    }

    #[test]
    fn palette_open_grows_inline_viewport_to_terminal_height() {
        // Opening the palette (Ctrl+P or any slash command that lands in a
        // detail view) takes over the screen so the overlay centers and
        // has room to render body content. Without this, the dialog
        // clamped to the 8-row bottom strip and clipped error messages.
        let mut app = empty_app();
        app.palette = Some(palette::open());

        assert_eq!(desired_inline_height(&app, 40), 40);
    }

    #[test]
    fn palette_dialog_area_centers_short_palette() {
        let mut app = empty_app();
        app.palette = Some(palette::open());

        let area = palette_dialog_area(&app, Rect::new(0, 0, 120, 80));

        assert_eq!(area.width, 72);
        assert_eq!(area.height, 32);
        assert_eq!(area.x, 24);
        assert_eq!(area.y, 24);
    }

    #[test]
    fn palette_closed_keeps_idle_inline_viewport_height() {
        let app = empty_app();
        assert_eq!(desired_inline_height(&app, 40), INLINE_HEIGHT_IDLE);
    }

    #[test]
    fn running_state_keeps_steady_inline_viewport_height() {
        let mut app = empty_app();
        let idle_height = desired_inline_height(&app, 40);
        app.run_detail = Some(RunDetail {
            run_id: "steady-running".into(),
            status: "running".into(),
            messages: vec![RuntimeMessageRow {
                role: "user".into(),
                content: "prompt".into(),
                attachments: Vec::new(),
                thinking: None,
                tool_calls: Vec::new(),
            }],
            events: Vec::new(),
            in_progress_text: None,
            in_progress_reasoning: None,
            tokens_used: None,
            context_window: None,
            started_at: Some(Instant::now()),
        });

        assert_eq!(
            desired_inline_height(&app, 40),
            idle_height,
            "run start should stay on the existing inline viewport"
        );
    }

    #[test]
    fn idle_inline_viewport_keeps_gap_above_composer() {
        let app = empty_app();
        let rows = render_rows(&app, 100, desired_inline_height(&app, 40));
        let first_composer_row = rows
            .iter()
            .position(|row| row.contains(super::super::theme::STRIPE_GLYPH))
            .expect("composer row");

        assert!(
            first_composer_row > 0,
            "composer should not render on the first viewport row"
        );
        assert!(
            rows[first_composer_row - 1].trim().is_empty(),
            "composer should keep one blank base row above its surface"
        );
    }

    #[test]
    fn multiline_composer_fits_steady_inline_viewport_height() {
        let mut app = empty_app();
        app.replace_composer("\n\n\n");

        assert_eq!(composer::height(&app), 8);
        assert_eq!(desired_inline_height(&app, 40), INLINE_HEIGHT_IDLE);
    }

    #[test]
    fn composer_first_multiline_row_keeps_gap_before_agent_footer() {
        let mut app = empty_app();
        app.replace_composer("\n");

        let rows = render_rows(&app, 100, desired_inline_height(&app, 40));
        let (cursor_row, _) = rendered_cursor_cell(&app, 100, desired_inline_height(&app, 40));
        let footer_row = rows
            .iter()
            .position(|row| row.contains("think:high"))
            .expect("agent footer row");

        assert_eq!(
            footer_row,
            cursor_row + 2,
            "composer should keep one blank row between cursor and agent labels"
        );
    }

    #[test]
    fn long_multiline_composer_keeps_cursor_row_visible() {
        let mut app = empty_app();
        app.replace_composer("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight");

        let text = render_to_string(&app, 100, desired_inline_height(&app, 40));
        let (_, cursor_symbol) = rendered_cursor_cell(&app, 100, desired_inline_height(&app, 40));

        assert!(
            text.contains("eight"),
            "composer should render the last input row with the cursor"
        );
        assert_eq!(
            cursor_symbol, " ",
            "cursor at end of line should render as a highlighted trailing cell"
        );
        assert!(
            !text.contains("one"),
            "composer should scroll to the bottom slice once input exceeds visible rows"
        );
    }

    #[test]
    fn long_multiline_composer_keeps_moved_cursor_row_visible() {
        let mut app = empty_app();
        app.replace_composer("one\ntwo\nthree\nfour\nfive\nsix\nseven\neight");
        app.composer_cursor = "one".len();

        let text = render_to_string(&app, 100, desired_inline_height(&app, 40));

        assert!(
            text.contains("one"),
            "composer should render the moved cursor row"
        );
        assert!(
            !text.contains("eight"),
            "composer should scroll to the cursor row instead of pinning to the bottom"
        );
    }

    #[test]
    fn inline_viewport_omits_box_drawing_borders() {
        let app = empty_app();
        let text = render_to_string(&app, 100, desired_inline_height(&app, 40));
        for symbol in [
            "\u{2500}", "\u{2502}", "\u{2510}", "\u{2514}", "\u{2518}", "\u{250C}",
        ] {
            assert!(
                !text.contains(symbol),
                "rendered viewport contains box-drawing glyph {symbol:?}",
            );
        }
    }

    #[test]
    fn short_viewport_keeps_composer_and_footer_visible() {
        let mut app = empty_app();
        app.replace_composer("one\ntwo\nthree\nfour\nfive");
        let text = render_to_string(&app, 60, 4);
        assert!(
            text.contains("think:high"),
            "short viewport should keep the composer agent footer visible"
        );
        assert!(
            text.contains("ctrl+p /commands"),
            "short viewport should keep the global footer visible"
        );
    }

    #[test]
    fn composer_shows_filename_for_attachment_path_input() {
        let path = temp_attachment_path("Screenshot 2026-05-19 at 6.22.51 PM.png");
        let mut app = empty_app();
        app.replace_composer(shell_escape_path(&path));

        let text = render_to_string(&app, 120, desired_inline_height(&app, 40));

        assert!(text.contains("[Screenshot 2026-05-19 at 6.22.51 PM.png]"));
        assert!(!text.contains(path.parent().unwrap().to_string_lossy().as_ref()));
        assert!(!slash::is_visible(&app));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn composer_highlights_entire_attachment_filename_at_cursor() {
        let path = temp_attachment_path("Screenshot 2026-05-19 at 5.34.30 PM.png");
        let mut app = empty_app();
        app.replace_composer(shell_escape_path(&path));

        let highlighted = highlighted_text(&app, 140, desired_inline_height(&app, 40));

        assert_eq!(highlighted, "[Screenshot 2026-05-19 at 5.34.30 PM.png]");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn backspace_removes_entire_attachment_path_token() {
        let globals = test_only_globals();
        let path = temp_attachment_path("Screenshot 2026-05-19 at 5.34.30 PM.png");
        let mut app = empty_app();
        app.replace_composer(format!("describe {}", shell_escape_path(&path)));
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, key(KeyCode::Backspace), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert_eq!(app.composer, "describe ");
        assert_eq!(app.composer_cursor, "describe ".len());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn composer_shows_filename_for_escaped_path_before_file_probe_succeeds() {
        let mut app = empty_app();
        app.replace_composer("/Users/sn0w/Desktop/Screenshot\\ 2026-05-19\\ at\\ 6.00.17\\ PM.png");

        let text = render_to_string(&app, 120, desired_inline_height(&app, 40));

        assert!(text.contains("[Screenshot 2026-05-19 at 6.00.17 PM.png]"));
        assert!(!text.contains("/Users/sn0w/Desktop"));
        assert!(!slash::is_visible(&app));
    }

    #[test]
    fn composer_brackets_complete_attachment_filename_with_timestamp() {
        let mut app = empty_app();
        app.replace_composer("/Users/sn0w/Desktop/Screenshot\\ 2026-05-19\\ at\\ 5.34.30 PM.png");

        let text = render_to_string(&app, 120, desired_inline_height(&app, 40));

        assert!(text.contains("[Screenshot 2026-05-19 at 5.34.30 PM.png]"));
        assert!(!text.contains("[Screenshot 2026-05-19 at 5.34.30] PM.png"));
        assert!(!text.contains("/Users/sn0w/Desktop"));
    }

    #[test]
    fn composer_preserves_prompt_text_while_hiding_multiple_attachment_paths() {
        let first = temp_attachment_path("first image.png");
        let second = temp_attachment_path("second image.png");
        let mut app = empty_app();
        app.replace_composer(format!(
            "compare {} with {}",
            shell_escape_path(&first),
            shell_escape_path(&second)
        ));

        let text = render_to_string(&app, 160, desired_inline_height(&app, 40));

        assert!(text.contains("compare [first image.png] with [second image.png]"));
        assert!(!text.contains(first.parent().unwrap().to_string_lossy().as_ref()));
        assert!(!text.contains(second.parent().unwrap().to_string_lossy().as_ref()));
        let _ = std::fs::remove_file(first);
        let _ = std::fs::remove_file(second);
    }

    #[test]
    fn slash_command_input_still_shows_suggestions() {
        let mut app = empty_app();
        app.replace_composer("/session");

        assert!(slash::is_visible(&app));
    }

    #[test]
    fn pasted_attachment_path_stages_file_without_inserting_full_path() {
        let adapter = AttachmentStageAdapter::default();
        let calls = Arc::clone(&adapter.calls);
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let path = temp_attachment_path("image picker upload.png");
        let mut app = empty_app();
        app.project.project_id = Some("project-1".into());
        app.project.registered = true;

        let outcome = handle_paste(&mut app, &shell_escape_path(&path), &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(app.composer.is_empty());
        assert_eq!(app.pending_attachments.len(), 1);
        assert_eq!(
            app.pending_attachments[0].staged.original_name,
            "image picker upload.png"
        );
        let calls = calls.lock().expect("recorded calls");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].first().map(String::as_str), Some("attachment"));
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn pasting_second_attachment_preserves_existing_prompt_text() {
        let adapter = AttachmentStageAdapter::default();
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let first = temp_attachment_path("first upload.png");
        let second = temp_attachment_path("second upload.png");
        let mut app = empty_app();
        app.project.project_id = Some("project-1".into());
        app.project.registered = true;

        handle_paste(&mut app, &shell_escape_path(&first), &globals);
        app.replace_composer("describe these");
        handle_paste(&mut app, &shell_escape_path(&second), &globals);

        assert_eq!(app.composer, "describe these");
        assert_eq!(app.pending_attachments.len(), 2);
        assert_eq!(
            app.pending_attachments
                .iter()
                .map(|attachment| attachment.staged.original_name.as_str())
                .collect::<Vec<_>>(),
            vec!["first upload.png", "second upload.png"]
        );
        let _ = std::fs::remove_file(first);
        let _ = std::fs::remove_file(second);
    }

    #[test]
    fn enter_on_attachment_path_stages_file_instead_of_slash_command() {
        let adapter = AttachmentStageAdapter::default();
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let path = temp_attachment_path("absolute path image.png");
        let mut app = empty_app();
        app.project.project_id = Some("project-1".into());
        app.project.registered = true;
        app.replace_composer(shell_escape_path(&path));
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, enter_key(), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(app.palette.is_none());
        assert!(app.composer.is_empty());
        assert_eq!(app.pending_attachments.len(), 1);
        assert_eq!(
            app.pending_attachments[0].staged.original_name,
            "absolute path image.png"
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn enter_on_prompt_with_multiple_attachment_paths_sends_prompt_and_attachments() {
        let adapter = AttachmentStageAdapter::default();
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let first = temp_attachment_path("first send.png");
        let second = temp_attachment_path("second send.png");
        let mut app = empty_app();
        app.project.project_id = Some("project-1".into());
        app.project.registered = true;
        app.providers = vec![ProviderRow {
            provider_id: "openai_codex".into(),
            default_model: "gpt-5.5".into(),
            credential_kind: "app_session".into(),
        }];
        app.replace_composer(format!(
            "describe these {} {}",
            shell_escape_path(&first),
            shell_escape_path(&second)
        ));
        let mut active_run = None;

        let outcome = dispatch_key(&mut app, enter_key(), &mut active_run, &globals);

        assert!(matches!(outcome, KeyOutcome::Continue));
        assert!(active_run.is_some());
        let user_message = &app
            .run_detail
            .as_ref()
            .expect("run detail")
            .messages
            .first()
            .expect("user message");
        assert_eq!(user_message.content.trim(), "describe these");
        assert_eq!(user_message.attachments.len(), 2);
        assert!(app.composer.is_empty());
        let _ = std::fs::remove_file(first);
        let _ = std::fs::remove_file(second);
    }

    #[test]
    fn attachments_palette_shows_filename_without_source_path() {
        let mut app = empty_app();
        app.pending_attachments.push(PendingAttachment {
            index: 1,
            source_path: PathBuf::from("/Users/sn0w/Desktop/Screenshot 2026-05-19.png"),
            staged: TuiStagedAttachmentDto {
                kind: "image".into(),
                absolute_path: "/tmp/xero/staged/screenshot.png".into(),
                media_type: "image/png".into(),
                original_name: "Screenshot 2026-05-19.png".into(),
                size_bytes: 42_000,
                width: None,
                height: None,
            },
        });

        let outcome = handle_attachments_command(&mut app);

        assert!(matches!(outcome, KeyOutcome::Continue));
        match app.palette.as_ref().expect("attachments detail") {
            PaletteState::Detail(detail) => match &detail.data {
                palette::DetailData::Body(lines) => {
                    let body = lines.join("\n");
                    assert!(body.contains("Screenshot 2026-05-19.png"));
                    assert!(!body.contains("/Users/sn0w/Desktop"));
                }
                _ => panic!("expected attachment body detail"),
            },
            PaletteState::Browse(_) => panic!("expected attachments detail"),
        }
    }

    #[test]
    fn terminal_resize_clear_runs_when_contracting() {
        assert!(should_clear_previous_inline_viewport(
            TerminalSize {
                width: 100,
                height: 24,
            },
            TerminalSize {
                width: 100,
                height: 12,
            },
        ));
    }

    #[test]
    fn terminal_resize_recalculates_inline_viewport_size() {
        let app = empty_app();
        let mut terminal = Terminal::with_options(
            TestBackend::new(100, 24),
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_HEIGHT_RUNNING),
            },
        )
        .expect("test backend");

        terminal.draw(|frame| render(frame, &app)).expect("draw");
        terminal.backend_mut().resize(52, 10);
        let mut terminal_size = TerminalSize {
            width: 100,
            height: 24,
        };
        sync_terminal_size(&mut terminal, &mut terminal_size, INLINE_HEIGHT_RUNNING)
            .expect("sync resized terminal");
        let completed = terminal.draw(|frame| render(frame, &app)).expect("draw");

        assert_eq!(completed.area.width, 52);
        assert_eq!(completed.area.height, 10);
        let buffer = terminal.backend().buffer();
        assert_eq!(buffer.area.width, 52);
        assert_eq!(buffer.area.height, 10);
        let text = buffer_to_string(buffer);
        assert!(
            text.contains("ctrl+p /commands"),
            "resized frame should redraw footer at the new width"
        );
    }

    #[test]
    fn pending_scrollback_uses_resized_terminal_width() {
        let mut app = empty_app();
        app.welcome_committed = true;
        app.run_detail = Some(RunDetail {
            run_id: "tui-resize-test".into(),
            status: "completed".into(),
            messages: vec![
                RuntimeMessageRow {
                    role: "user".into(),
                    content: "resize prompt".into(),
                    attachments: Vec::new(),
                    thinking: None,
                    tool_calls: Vec::new(),
                },
                RuntimeMessageRow {
                    role: "assistant".into(),
                    content: "resize reply".into(),
                    attachments: Vec::new(),
                    thinking: None,
                    tool_calls: Vec::new(),
                },
            ],
            events: Vec::new(),
            in_progress_text: None,
            in_progress_reasoning: None,
            tokens_used: None,
            context_window: None,
            started_at: None,
        });
        let mut terminal = Terminal::with_options(
            TestBackend::new(100, 16),
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_HEIGHT_RUNNING),
            },
        )
        .expect("test backend");

        terminal.draw(|frame| render(frame, &app)).expect("draw");
        terminal.backend_mut().resize(48, 12);
        let mut terminal_size = TerminalSize {
            width: 100,
            height: 16,
        };
        sync_terminal_size(&mut terminal, &mut terminal_size, INLINE_HEIGHT_RUNNING)
            .expect("sync resized terminal");
        commit_pending_history(&mut terminal, &mut app).expect("commit scrollback");

        assert_eq!(app.committed_messages, 2);
        assert_eq!(terminal.backend().scrollback().area.width, 48);
        let history = format!(
            "{}{}",
            buffer_to_string(terminal.backend().scrollback()),
            buffer_to_string(terminal.backend().buffer())
        );
        assert!(
            history.contains("resize prompt"),
            "terminal history should contain user prompt after resize"
        );
        assert!(
            history.contains("resize reply"),
            "terminal history should contain assistant reply after resize"
        );
    }

    #[test]
    fn replayed_welcome_uses_current_terminal_width() {
        let mut app = empty_app();
        let mut terminal = Terminal::with_options(
            TestBackend::new(80, 24),
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_HEIGHT_IDLE),
            },
        )
        .expect("test backend");
        commit_pending_history(&mut terminal, &mut app).expect("commit welcome");
        assert_eq!(app.committed_scrollback.len(), 1);

        let mut resized = Terminal::with_options(
            TestBackend::new(120, 24),
            TerminalOptions {
                viewport: Viewport::Inline(app.inline_height),
            },
        )
        .expect("resized test backend");
        replay_committed_scrollback(&mut resized, &mut app).expect("replay welcome");

        let logo_row = terminal_history_rows(&resized)
            .into_iter()
            .find(|row| row.contains("██╗  ██╗"))
            .expect("logo row");
        let logo_width = "██╗  ██╗ ███████╗ ██████╗   ██████╗ ".chars().count();
        let leading_spaces = logo_row.chars().take_while(|ch| *ch == ' ').count();
        assert_eq!(
            leading_spaces,
            (120usize - logo_width) / 2,
            "welcome logo should be centered at the replayed width"
        );
    }

    #[test]
    fn replayed_scrollback_messages_use_current_terminal_width() {
        let mut app = empty_app();
        app.welcome_committed = true;
        app.run_detail = Some(RunDetail {
            run_id: "tui-replay-width".into(),
            status: "completed".into(),
            messages: vec![RuntimeMessageRow {
                role: "user".into(),
                content: "resize prompt".into(),
                attachments: Vec::new(),
                thinking: None,
                tool_calls: Vec::new(),
            }],
            events: Vec::new(),
            in_progress_text: None,
            in_progress_reasoning: None,
            tokens_used: None,
            context_window: None,
            started_at: None,
        });
        let mut terminal = Terminal::with_options(
            TestBackend::new(100, 16),
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_HEIGHT_IDLE),
            },
        )
        .expect("test backend");
        commit_pending_history(&mut terminal, &mut app).expect("commit prompt");
        assert_eq!(app.committed_scrollback.len(), 1);

        let mut resized = Terminal::with_options(
            TestBackend::new(48, 16),
            TerminalOptions {
                viewport: Viewport::Inline(app.inline_height),
            },
        )
        .expect("resized test backend");
        replay_committed_scrollback(&mut resized, &mut app).expect("replay prompt");

        let prompt_row = terminal_history_rows(&resized)
            .into_iter()
            .find(|row| row.contains("resize prompt"))
            .expect("prompt row");
        assert_eq!(
            prompt_row.chars().count(),
            48,
            "replayed prompt row should be repainted to the resized width"
        );
        assert!(
            prompt_row.contains(super::super::theme::STRIPE_GLYPH),
            "replayed prompt row should keep the composer-style stripe"
        );
    }

    #[test]
    fn streaming_response_reveals_only_completed_rows() {
        let mut app = empty_app();
        app.welcome_committed = true;
        app.run_detail = Some(RunDetail {
            run_id: "row-stream-response".into(),
            status: "running".into(),
            messages: vec![RuntimeMessageRow {
                role: "user".into(),
                content: "prompt".into(),
                attachments: Vec::new(),
                thinking: None,
                tool_calls: Vec::new(),
            }],
            events: Vec::new(),
            in_progress_text: Some("alpha beta".into()),
            in_progress_reasoning: None,
            tokens_used: None,
            context_window: None,
            started_at: Some(Instant::now()),
        });
        let mut terminal = Terminal::with_options(
            TestBackend::new(18, 12),
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_HEIGHT_RUNNING),
            },
        )
        .expect("test backend");

        commit_pending_history(&mut terminal, &mut app).expect("commit short stream");
        let history = terminal_history_to_string(&terminal);
        assert!(
            !history.contains("alpha beta"),
            "unfinished response row should stay hidden"
        );

        app.run_detail.as_mut().unwrap().in_progress_text =
            Some("alpha beta gamma delta epsilon".into());
        commit_pending_history(&mut terminal, &mut app).expect("commit wrapped stream");
        let history = terminal_history_to_string(&terminal);
        assert!(
            history.contains("alpha beta gamma"),
            "completed response row should be revealed"
        );
        assert!(
            !history.contains("delta epsilon"),
            "unfinished trailing response row should stay hidden"
        );
    }

    #[test]
    fn streaming_reasoning_reveals_only_completed_rows() {
        let mut app = empty_app();
        app.welcome_committed = true;
        app.run_detail = Some(RunDetail {
            run_id: "row-stream-reasoning".into(),
            status: "running".into(),
            messages: vec![RuntimeMessageRow {
                role: "user".into(),
                content: "prompt".into(),
                attachments: Vec::new(),
                thinking: None,
                tool_calls: Vec::new(),
            }],
            events: Vec::new(),
            in_progress_text: None,
            in_progress_reasoning: Some("think alpha".into()),
            tokens_used: None,
            context_window: None,
            started_at: Some(Instant::now()),
        });
        let mut terminal = Terminal::with_options(
            TestBackend::new(18, 12),
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_HEIGHT_RUNNING),
            },
        )
        .expect("test backend");

        commit_pending_history(&mut terminal, &mut app).expect("commit short reasoning");
        let history = terminal_history_to_string(&terminal);
        assert!(
            !history.contains("think alpha"),
            "unfinished reasoning row should stay hidden"
        );

        app.run_detail.as_mut().unwrap().in_progress_reasoning =
            Some("think alpha beta gamma delta".into());
        commit_pending_history(&mut terminal, &mut app).expect("commit wrapped reasoning");
        let history = terminal_history_to_string(&terminal);
        assert!(
            history.contains("think alpha"),
            "completed reasoning row should be revealed"
        );
        assert!(
            !history.contains("gamma delta"),
            "unfinished trailing reasoning row should stay hidden"
        );
    }

    #[test]
    fn inline_viewport_hides_token_stream_tail() {
        let mut app = empty_app();
        app.run_detail = Some(RunDetail {
            run_id: "inline-stream-tail".into(),
            status: "running".into(),
            messages: vec![RuntimeMessageRow {
                role: "user".into(),
                content: "prompt".into(),
                attachments: Vec::new(),
                thinking: None,
                tool_calls: Vec::new(),
            }],
            events: Vec::new(),
            in_progress_text: Some("partial response tail".into()),
            in_progress_reasoning: Some("partial reasoning tail".into()),
            tokens_used: None,
            context_window: None,
            started_at: Some(Instant::now()),
        });

        let text = render_to_string(&app, 80, 10);

        assert!(
            text.contains("Thinking"),
            "running viewport should keep spinner label"
        );
        assert!(
            !text.contains("partial response tail"),
            "inline viewport should not expose response token tail"
        );
        assert!(
            !text.contains("partial reasoning tail"),
            "inline viewport should not expose reasoning token tail"
        );
    }

    #[test]
    fn running_inline_viewport_keeps_row_gap_around_thinking_indicator() {
        let mut app = empty_app();
        app.run_detail = Some(RunDetail {
            run_id: "inline-thinking-spacing".into(),
            status: "running".into(),
            messages: vec![RuntimeMessageRow {
                role: "user".into(),
                content: "prompt".into(),
                attachments: Vec::new(),
                thinking: None,
                tool_calls: Vec::new(),
            }],
            events: Vec::new(),
            in_progress_text: None,
            in_progress_reasoning: None,
            tokens_used: None,
            context_window: None,
            started_at: Some(Instant::now()),
        });

        let rows = render_rows(&app, 80, desired_inline_height(&app, 40));
        let thinking_row = rows
            .iter()
            .position(|row| row.contains("Thinking"))
            .expect("thinking row");

        assert!(thinking_row > 0, "thinking row should keep a row above it");
        assert!(
            rows[thinking_row - 1].trim().is_empty(),
            "row above the thinking indicator should stay blank"
        );
        assert!(
            thinking_row + 1 < rows.len(),
            "thinking row should keep a row below it"
        );
        assert!(
            rows[thinking_row + 1].trim().is_empty(),
            "row below the thinking indicator should stay blank"
        );
    }

    #[test]
    fn resize_growth_clears_previous_inline_viewport_rows() {
        let app = empty_app();
        let mut terminal = Terminal::with_options(
            TestBackend::new(100, 16),
            TerminalOptions {
                viewport: Viewport::Inline(INLINE_HEIGHT_IDLE),
            },
        )
        .expect("test backend");
        let mut terminal_size = current_terminal_size(&terminal).expect("initial terminal size");

        terminal.draw(|frame| render(frame, &app)).expect("draw");
        terminal.backend_mut().resize(100, 10);
        sync_terminal_size(&mut terminal, &mut terminal_size, INLINE_HEIGHT_IDLE)
            .expect("sync shrink");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("draw shrunken viewport");

        terminal.backend_mut().resize(100, 16);
        sync_terminal_size(&mut terminal, &mut terminal_size, INLINE_HEIGHT_IDLE)
            .expect("sync growth");
        terminal
            .draw(|frame| render(frame, &app))
            .expect("draw grown viewport");

        let rows = buffer_rows(terminal.backend().buffer());
        let inline_rows = INLINE_HEIGHT_IDLE as usize;
        let top_viewport = rows[..inline_rows.min(rows.len())].join("\n");
        let bottom_start = rows.len().saturating_sub(inline_rows);
        let bottom_viewport = rows[bottom_start..].join("\n");
        let (inactive_rows, viewport) = if top_viewport.contains("ctrl+p /commands") {
            (rows[inline_rows.min(rows.len())..].join("\n"), top_viewport)
        } else {
            (rows[..bottom_start].join("\n"), bottom_viewport)
        };
        assert!(
            !inactive_rows.contains(super::super::theme::STRIPE_GLYPH),
            "old composer stripe should not remain outside the grown inline viewport"
        );
        assert!(
            !inactive_rows.contains("ctrl+p /commands"),
            "old footer text should not remain above the grown inline viewport"
        );
        assert!(
            viewport.contains("ctrl+p /commands"),
            "grown viewport should redraw the active footer"
        );
    }

    #[test]
    fn welcome_banner_lines_contain_ascii_logo_and_fill_target_height() {
        // The big logo only appears in scrollback now — call the helper
        // directly to confirm it renders and pads to the requested height.
        let target_height = 24;
        let lines =
            super::super::transcript::welcome_banner_lines(80, target_height, "9.8.7", None);
        assert!(
            lines.len() >= target_height as usize,
            "welcome banner should pad to at least target_height rows ({} got {})",
            target_height,
            lines.len()
        );
        let joined: String = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect();
        assert!(
            joined.contains('\u{2588}'),
            "welcome banner missing ASCII logo block glyphs"
        );
        assert!(joined.contains("[BETA]"), "welcome banner missing beta tag");
        assert!(
            joined.contains("v9.8.7"),
            "welcome banner missing version label, got: {joined:?}"
        );
        assert!(
            joined.contains("not signed in"),
            "welcome banner should prompt sign-in when no handle is given"
        );
    }

    #[test]
    fn load_signed_in_handle_reads_camelcase_identity_file() {
        let dir = std::env::temp_dir().join(format!(
            "xero-identity-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("desktop-identity.json");
        std::fs::write(
            &path,
            r#"{"accountId":"a","desktopDeviceId":"d","desktopJwt":"j","githubLogin":"octo"}"#,
        )
        .unwrap();
        assert_eq!(super::load_signed_in_handle(&path), Some("@octo".into()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_signed_in_handle_returns_none_when_file_missing() {
        let missing = std::env::temp_dir().join("xero-identity-missing-file.json");
        let _ = std::fs::remove_file(&missing);
        assert!(super::load_signed_in_handle(&missing).is_none());
    }

    #[test]
    fn welcome_banner_lines_show_github_handle_when_signed_in() {
        let lines = super::super::transcript::welcome_banner_lines(80, 24, "1.0.0", Some("@octo"));
        let joined: String = lines
            .iter()
            .flat_map(|line| line.spans.iter().map(|span| span.content.as_ref()))
            .collect();
        assert!(
            joined.contains("@octo"),
            "welcome banner missing signed-in handle, got: {joined:?}"
        );
        assert!(
            !joined.contains("not signed in"),
            "welcome banner should hide sign-in hint when authenticated"
        );
    }

    #[test]
    fn scrollback_includes_user_prompt_and_assistant_reply() {
        // Run a user + assistant pair through the scrollback renderer and
        // confirm the visible text shows up.
        let user = RuntimeMessageRow {
            role: "user".into(),
            content: "1+1".into(),
            attachments: Vec::new(),
            thinking: None,
            tool_calls: Vec::new(),
        };
        let assistant = RuntimeMessageRow {
            role: "assistant".into(),
            content: "2".into(),
            attachments: Vec::new(),
            thinking: None,
            tool_calls: Vec::new(),
        };
        let lines: Vec<String> = [&user, &assistant]
            .iter()
            .flat_map(|message| {
                super::super::transcript::message_lines(message, 80)
                    .into_iter()
                    .map(|line| {
                        line.spans
                            .iter()
                            .map(|span| span.content.to_string())
                            .collect::<String>()
                    })
            })
            .collect();
        let joined = lines.join("\n");
        assert!(joined.contains("1+1"), "missing user prompt in scrollback");
        assert!(
            joined.contains('2'),
            "missing assistant reply in scrollback"
        );
    }

    #[test]
    fn cycle_agent_advances_then_wraps() {
        let mut app = empty_app();
        let starting = app.selected_agent_label().to_owned();
        for _ in 0..app.agents.len() {
            app.cycle_agent();
        }
        assert_eq!(app.selected_agent_label(), starting);
    }

    #[test]
    fn thinking_effort_cycle_wraps_after_x_high() {
        let mut effort = ThinkingEffort::None;
        for expected in [
            ThinkingEffort::Minimal,
            ThinkingEffort::Low,
            ThinkingEffort::Medium,
            ThinkingEffort::High,
            ThinkingEffort::XHigh,
            ThinkingEffort::None,
        ] {
            effort = effort.next();
            assert_eq!(effort, expected);
        }
    }

    #[test]
    fn start_prompt_job_passes_selected_agent_and_thinking_to_agent_exec() {
        let adapter = RecordingAdapter::default();
        let calls = Arc::clone(&adapter.calls);
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let mut app = empty_app();
        app.project.project_id = Some("project-1".into());
        app.providers = vec![ProviderRow {
            provider_id: "openai_codex".into(),
            default_model: "gpt-5.5".into(),
            credential_kind: "app_session".into(),
        }];
        app.selected_agent = app
            .agents
            .iter()
            .position(|agent| agent.definition_id == "ask")
            .expect("ask agent");
        app.thinking_effort = ThinkingEffort::XHigh;
        app.active_session_id = Some("session-ask".into());

        let job = start_prompt_job(&globals, &mut app, "What is this project about?")
            .expect("start prompt");
        let result = job.receiver.recv().expect("prompt result");
        assert!(result.is_ok());

        let calls = calls.lock().expect("recorded calls");
        let args = calls.last().expect("agent exec call");
        assert!(has_arg_pair(args, "--runtime-agent-id", "ask"));
        assert!(has_arg_pair(args, "--agent-definition-id", "ask"));
        assert!(has_arg_pair(args, "--thinking-effort", "x_high"));
        assert!(has_arg_pair(args, "--session-id", "session-ask"));
        assert!(has_arg_pair(args, "--project-id", "project-1"));
    }

    #[test]
    fn start_prompt_job_does_not_render_run_status_in_footer() {
        let adapter = RecordingAdapter::default();
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let mut app = empty_app();
        app.status = Some("Previous footer status".into());
        app.providers = vec![ProviderRow {
            provider_id: "openai_codex".into(),
            default_model: "gpt-5.5".into(),
            credential_kind: "app_session".into(),
        }];

        let job = start_prompt_job(&globals, &mut app, "What is this project about?")
            .expect("start prompt");

        assert!(app.status.is_none());
        let viewport = render_to_string(&app, 100, desired_inline_height(&app, 40));
        assert!(
            !viewport.contains("started via"),
            "footer should not render run-start status"
        );
        assert!(
            !viewport.contains("Previous footer status"),
            "run start should clear stale footer status"
        );
        let result = job.receiver.recv().expect("prompt result");
        assert!(result.is_ok());
    }

    #[test]
    fn completed_prompt_job_does_not_render_run_status_in_footer() {
        let globals = test_only_globals();
        let mut app = empty_app();
        app.status = Some("Previous footer status".into());
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(Ok(json!({
                "snapshot": {
                    "runId": "tui-run-test",
                    "status": "completed",
                    "messages": [],
                    "events": []
                }
            })))
            .expect("send prompt result");
        let mut active_run = Some(PromptJob {
            project_id: None,
            agent_session_id: "session-test".into(),
            run_id: "tui-run-test".into(),
            clears_pending_attachments: false,
            receiver,
        });

        poll_active_run(&globals, &mut app, &mut active_run);

        assert!(active_run.is_none());
        assert!(app.status.is_none());
        let viewport = render_to_string(&app, 100, desired_inline_height(&app, 40));
        assert!(
            !viewport.contains("Run completed: tui-run-test"),
            "footer should not render run-completed status"
        );
        assert!(
            !viewport.contains("Previous footer status"),
            "run completion should clear stale footer status"
        );
    }

    #[test]
    fn completed_project_prompt_auto_names_session_before_publish() {
        let adapter = RecordingAdapter::default();
        let calls = Arc::clone(&adapter.calls);
        let mut globals = test_only_globals();
        globals.tui_adapter = Some(Arc::new(adapter));
        let mut app = empty_app();
        let (sender, receiver) = std::sync::mpsc::channel();
        sender
            .send(Ok(json!({
                "snapshot": {
                    "projectId": "project-1",
                    "agentSessionId": "session-test",
                    "runId": "tui-run-test",
                    "status": "completed",
                    "messages": [],
                    "events": []
                }
            })))
            .expect("send prompt result");
        let mut active_run = Some(PromptJob {
            project_id: Some("project-1".into()),
            agent_session_id: "session-test".into(),
            run_id: "tui-run-test".into(),
            clears_pending_attachments: false,
            receiver,
        });

        poll_active_run(&globals, &mut app, &mut active_run);

        assert!(active_run.is_none());
        let calls = calls.lock().expect("recorded calls");
        assert!(
            calls.iter().any(|args| {
                args.iter().map(String::as_str).collect::<Vec<_>>()
                    == vec![
                        "session",
                        "auto-name",
                        "--project-id",
                        "project-1",
                        "session-test",
                    ]
            }),
            "expected session auto-name call, got {calls:?}"
        );
    }

    #[test]
    fn one_plus_one_golden_inline_viewport_and_scrollback() {
        // The viewport asserts the composer + footer state; the scroll-
        // back assertions cover the conversation content emitted via
        // `transcript::message_lines` / `tool_pill_lines`.
        let mut app = empty_app();
        app.fake_provider_fixture = true;
        let user = RuntimeMessageRow {
            role: "user".into(),
            content: "1+1".into(),
            attachments: Vec::new(),
            thinking: None,
            tool_calls: Vec::new(),
        };
        let assistant = RuntimeMessageRow {
            role: "assistant".into(),
            content: "2".into(),
            attachments: Vec::new(),
            thinking: Some("The user is asking a simple math question.".into()),
            tool_calls: vec![ToolCallRow {
                name: "calc".into(),
                tool_call_id: None,
                detail: None,
                completed_duration: Some(Duration::from_millis(4_100)),
            }],
        };
        app.run_detail = Some(RunDetail {
            run_id: "tui-test-run".into(),
            status: "completed".into(),
            messages: vec![user.clone(), assistant.clone()],
            events: Vec::new(),
            in_progress_text: None,
            in_progress_reasoning: None,
            tokens_used: Some(36_400),
            context_window: Some(1_000_000),
            started_at: None,
        });

        let viewport = render_to_string(&app, 100, desired_inline_height(&app, 40));
        assert!(viewport.contains("Ask"), "viewport missing agent label");
        assert!(
            viewport.contains("think:"),
            "viewport missing thinking-effort indicator",
        );
        assert!(
            viewport.contains("ctrl+p /commands"),
            "viewport missing footer palette hint",
        );
        assert!(viewport.contains("36.4K"), "viewport missing token count");
        assert!(viewport.contains("(4%)"), "viewport missing token percent");

        let scrollback_text: String = [&user, &assistant]
            .iter()
            .flat_map(|message| {
                let mut lines = super::super::transcript::message_lines(message, 100);
                lines.extend(super::super::transcript::tool_pill_lines(
                    &message.tool_calls,
                ));
                lines.into_iter().map(|line| {
                    line.spans
                        .iter()
                        .map(|span| span.content.to_string())
                        .collect::<String>()
                })
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(scrollback_text.contains("1+1"), "scrollback missing prompt");
        assert!(scrollback_text.contains('2'), "scrollback missing answer");
        // Reasoning now lives in scrollback without the "Thinking:"
        // label — the stripe + italic dim styling carries that signal.
        // Verify the reasoning *content* still made it through.
        assert!(
            scrollback_text.contains("simple math question"),
            "scrollback missing reasoning content",
        );
        assert!(
            scrollback_text.contains(super::super::theme::TOOL_DOT),
            "scrollback missing tool pill glyph",
        );
    }
}
