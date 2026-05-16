//! Single-column app state and the event/render loop.

use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use crossterm::{
    cursor::MoveTo,
    event::{
        self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers, KeyboardEnhancementFlags,
        PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{
        disable_raw_mode, enable_raw_mode, Clear as CrosstermClear, ClearType as CrosstermClearType,
    },
};
use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend},
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Position, Rect, Size},
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
    pub thinking: Option<String>,
    pub tool_calls: Vec<ToolCallRow>,
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
/// to a static seed when no project is selected so a fresh `xero tui` in
/// an unregistered directory still has something to cycle through.
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

/// Mirrors `ProviderModelThinkingEffortDto` on the desktop side so cycling
/// in the TUI feeds the same downstream provider plumbing. Persisted with
/// the snake-case `x_high` spelling the rest of the codebase uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThinkingEffort {
    Minimal,
    Low,
    Medium,
    High,
    XHigh,
}

impl ThinkingEffort {
    pub fn label(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "x_high",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Minimal => Self::Low,
            Self::Low => Self::Medium,
            Self::Medium => Self::High,
            Self::High => Self::XHigh,
            Self::XHigh => Self::Minimal,
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Some(match value {
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
    pub run_detail: Option<RunDetail>,
    pub active_run_id: Option<String>,
    pub active_session_id: Option<String>,
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
    /// back when the run goes idle so the composer stays flush against
    /// the previous response.
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
        let providers = load_providers(globals).unwrap_or_default();
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
            run_detail: None,
            active_run_id: None,
            active_session_id: None,
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
        self.committed_messages = 0;
        self.welcome_committed = false;
        self.streamed_text_rows = 0;
        self.streamed_reasoning_rows = 0;
        self.last_scrollback_line_blank = false;
        self.committed_scrollback.clear();
        self.tui_reset_requested = true;
    }

    fn take_tui_reset_requested(&mut self) -> bool {
        std::mem::take(&mut self.tui_reset_requested)
    }
}

struct PromptJob {
    project_id: Option<String>,
    run_id: String,
    receiver: Receiver<Result<JsonValue, CliError>>,
}

pub fn run_interactive(globals: GlobalOptions) -> Result<CliResponse, CliError> {
    let project = super::project::resolve(&globals);
    let mut app = App::new(&globals, project);
    // Probe streaming support once so the rest of the loop can adopt the
    // NDJSON path as soon as the backend ships `--stream`.
    let _ = runtime::mode(&globals);

    enable_raw_mode().map_err(tui_io_error)?;
    let keyboard_enhancement_pushed = push_keyboard_enhancements();
    let backend = CrosstermBackend::new(io::stdout());
    // Inline viewport: only the bottom rows belong to ratatui. Everything
    // we want to live in scrollback (welcome banner, user prompts,
    // assistant responses, tool pills) gets emitted via
    // `terminal.insert_before(...)`. The user's terminal then owns the
    // history and its native scrolling does the work — no manual scroll
    // state, no alt-screen tricks.
    let mut terminal = Terminal::with_options(
        backend,
        TerminalOptions {
            viewport: Viewport::Inline(app.inline_height),
        },
    )
    .map_err(tui_io_error)?;

    let result = event_loop(&globals, &mut terminal, &mut app);

    let keyboard_enhancement_result = pop_keyboard_enhancements(keyboard_enhancement_pushed);
    disable_raw_mode().map_err(tui_io_error)?;
    terminal.show_cursor().map_err(tui_io_error)?;
    // Drop the inline viewport so the cursor lands below it and the user
    // doesn't see the composer hanging around after we exit.
    terminal
        .clear()
        .and_then(|_| terminal.flush())
        .map_err(tui_io_error)?;
    println!();

    result?;
    keyboard_enhancement_result?;
    Ok(CliResponse {
        output_mode: globals.output_mode,
        text: String::new(),
        json: json!({ "kind": "tui", "status": "closed" }),
        emit: false,
    })
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

/// Inline viewport heights. We use a few sizes:
///   - `INLINE_HEIGHT_RUNNING` (10 rows): hosts the streaming spinner
///     and in-flight tool row above the composer block.
///   - `INLINE_HEIGHT_SLASH` (12 rows): lets composer-owned slash command
///     suggestions behave like an inline select list.
///   - `INLINE_HEIGHT_IDLE` (8 rows): composer + one row of breathing
///     room above it + composer-to-footer gap + footer. Drops the
///     reserved spinner rows but keeps a visual buffer so the
///     composer doesn't clamp directly against the last response.
pub const INLINE_HEIGHT_RUNNING: u16 = 10;
pub const INLINE_HEIGHT_SLASH: u16 = 12;
pub const INLINE_HEIGHT_IDLE: u16 = 8;
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
    let composer_required_height = composer::height(app).saturating_add(2);
    let running = matches!(
        app.run_detail.as_ref().map(|detail| detail.status.as_str()),
        Some("running")
    );
    let fixed_height = match (running, slash::is_visible(app)) {
        (true, true) => INLINE_HEIGHT_RUNNING.max(INLINE_HEIGHT_SLASH),
        (true, false) => INLINE_HEIGHT_RUNNING,
        (false, true) => INLINE_HEIGHT_SLASH,
        (false, false) => INLINE_HEIGHT_IDLE,
    };
    fixed_height.max(composer_required_height)
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
    let mut terminal_size = current_terminal_size(terminal)?;
    loop {
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
        // state. When idle, we drop the spinner-area rows so the
        // composer sits flush against the last response.
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
            palette::KeyResult::Quit => KeyOutcome::Quit,
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
        }
        KeyCode::BackTab => {
            app.cycle_thinking_effort();
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
    ) {
        if !slash::is_visible(app) {
            slash::clamp_selection(app);
        }
    }
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

    if words.len() == 1 {
        if let Some(outcome) = palette::activate_command_by_id(&words[0], app, globals) {
            return palette_result_to_key_outcome(app, outcome);
        }
        if let Some(alias) = slash_dialog_alias(&words[0]) {
            if let Some(outcome) = palette::activate_command_by_id(alias, app, globals) {
                return palette_result_to_key_outcome(app, outcome);
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
) -> xero_remote_bridge::RemoteBridge<
    xero_remote_bridge::FileIdentityStore,
    xero_remote_bridge::FileSessionVisibilityStore,
> {
    let remote_dir = crate::cli_app_data_root(globals).join("remote");
    xero_remote_bridge::RemoteBridge::new(
        xero_remote_bridge::BridgeConfig::from_env_or_local("Xero TUI"),
        xero_remote_bridge::FileIdentityStore::new(remote_dir.join("desktop-identity.json")),
        xero_remote_bridge::FileSessionVisibilityStore::new(
            remote_dir.join("remote-visibility.json"),
        ),
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

fn palette_result_to_key_outcome(app: &mut App, result: palette::KeyResult) -> KeyOutcome {
    match result {
        palette::KeyResult::Continue => KeyOutcome::Continue,
        palette::KeyResult::Close { status } => {
            app.palette = None;
            if let Some(message) = status {
                app.status = Some(message);
            }
            KeyOutcome::Continue
        }
        palette::KeyResult::Quit => KeyOutcome::Quit,
    }
}

pub(crate) fn slash_dialog_alias(command: &str) -> Option<&'static str> {
    Some(match command {
        "session" | "sessions" => "sessions",
        "provider" | "providers" => "providers",
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
    let run_id = generate_id("tui-run");
    let project_id = app.project.project_id.clone();
    let agent_session_id = app.active_session_id.clone();
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
    if let Some(agent_session_id) = agent_session_id.as_deref() {
        owned.push("--session-id".into());
        owned.push(agent_session_id.to_owned());
    }

    let (sender, receiver) = mpsc::channel();
    let globals_clone = globals.clone();
    thread::spawn(move || {
        let borrowed = owned.iter().map(String::as_str).collect::<Vec<_>>();
        let _ = sender.send(invoke_json(&globals_clone, &borrowed));
    });

    app.status = Some(format!(
        "Run {} started via {} as {}.",
        run_id, provider_id, runtime_agent_id
    ));
    let started_at = Instant::now();
    app.run_detail = Some(RunDetail {
        run_id: run_id.clone(),
        status: "running".into(),
        messages: vec![RuntimeMessageRow {
            role: "user".into(),
            content: prompt.to_owned(),
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
        run_id,
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
            app.status = Some(format!("Run completed: {}", job.run_id));
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
        Ok(agents)
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

    let footer_height = u16::from(root.height > 0);
    let gap_height = u16::from(root.height > footer_height + 1);
    let composer_natural = composer::height(app);
    // Only insert a row of padding above the composer when the viewport
    // has at least one row to spare beyond the composer's natural height
    // — short viewports keep every row for the composer agent footer.
    let streaming_gap_height =
        u16::from(root.height > composer_natural + footer_height + gap_height);
    let composer_height = composer_natural.min(
        root.height
            .saturating_sub(footer_height + gap_height + streaming_gap_height),
    );
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Min(0),                       // streaming indicator
            Constraint::Length(streaming_gap_height), // padding above composer
            Constraint::Length(composer_height),      // composer
            Constraint::Length(gap_height),           // composer-to-footer gap
            Constraint::Length(footer_height),        // footer
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

    if app.palette.is_some() {
        let palette_area = palette_dialog_area(app, root);
        frame.render_widget(Clear, palette_area);
        palette::render(frame, palette_area, app);
    }
}

fn palette_dialog_area(app: &App, root: Rect) -> Rect {
    let available = root;
    let (width, height) = palette::desired_size(app, available);
    Rect {
        x: available.x + available.width.saturating_sub(width) / 2,
        y: available.y + available.height.saturating_sub(height) / 2,
        width: width.min(available.width),
        height: height.min(available.height),
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
    let app = App::new(globals, project);
    let mut terminal =
        Terminal::new(ratatui::backend::TestBackend::new(100, 32)).map_err(|error| {
            CliError::system_fault(
                "xero_tui_smoke_terminal_failed",
                format!("Could not create TUI test backend: {error}"),
            )
        })?;
    terminal
        .draw(|frame| render(frame, &app))
        .map_err(|error| {
            CliError::system_fault(
                "xero_tui_smoke_render_failed",
                format!("Could not render TUI smoke frame: {error}"),
            )
        })?;
    let buffer_text = terminal
        .backend()
        .buffer()
        .content
        .iter()
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
    app.status = Some(format!("Fake-provider smoke run completed: {}", run_id));

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
        run_detail: None,
        active_run_id: None,
        active_session_id: None,
        last_ctrl_c_at: None,
        committed_messages: 0,
        welcome_committed: false,
        tui_reset_requested: false,
        streamed_text_rows: 0,
        streamed_reasoning_rows: 0,
        inline_height: INLINE_HEIGHT_IDLE,
        last_scrollback_line_blank: false,
        committed_scrollback: Vec::new(),
        preferences_path: PathBuf::from("/tmp/xero-tui-test-prefs.json"),
        identity_path: PathBuf::from("/tmp/xero-tui-test-identity.json"),
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
            run_detail: None,
            active_run_id: None,
            active_session_id: None,
            last_ctrl_c_at: None,
            committed_messages: 0,
            welcome_committed: false,
            tui_reset_requested: false,
            streamed_text_rows: 0,
            streamed_reasoning_rows: 0,
            inline_height: INLINE_HEIGHT_IDLE,
            last_scrollback_line_blank: false,
            committed_scrollback: Vec::new(),
            preferences_path: PathBuf::from("/tmp/xero-tui-test-prefs.json"),
            identity_path: PathBuf::from("/tmp/xero-tui-test-identity.json"),
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
        let text = render_to_string(&app, 100, 8);
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
                "project-1".to_owned()
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
    fn slash_partial_enter_uses_selected_inline_suggestion() {
        let mut app = empty_app();
        app.replace_composer("/prov");

        let selected = slash::selected_submission(&app, "/prov");

        assert_eq!(selected.as_deref(), Some("/providers"));
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
    fn palette_closed_keeps_idle_inline_viewport_height() {
        let app = empty_app();
        assert_eq!(desired_inline_height(&app, 40), INLINE_HEIGHT_IDLE);
    }

    #[test]
    fn multiline_composer_grows_inline_viewport_past_idle_height() {
        let mut app = empty_app();
        app.replace_composer("\n\n\n");

        assert_eq!(composer::height(&app), 8);
        assert_eq!(desired_inline_height(&app, 40), 10);
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
        let text = render_to_string(&app, 100, 8);
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
                    thinking: None,
                    tool_calls: Vec::new(),
                },
                RuntimeMessageRow {
                    role: "assistant".into(),
                    content: "resize reply".into(),
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
            "xero-tui-identity-{}",
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
        let missing = std::env::temp_dir().join("xero-tui-identity-missing-file.json");
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
            thinking: None,
            tool_calls: Vec::new(),
        };
        let assistant = RuntimeMessageRow {
            role: "assistant".into(),
            content: "2".into(),
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
    fn default_agent_catalog_matches_tauri_seeds() {
        let catalog = default_agent_catalog();
        let labels: Vec<String> = catalog
            .iter()
            .map(|entry| entry.label().to_owned())
            .collect();
        // Same names users see in the desktop UI.
        for expected in ["Ask", "Plan", "Engineer", "Debug", "Agent Create", "Agent"] {
            assert!(
                labels.iter().any(|label| label == expected),
                "missing seeded agent `{expected}` (have: {labels:?})",
            );
        }
    }

    #[test]
    fn startup_agent_selection_prefers_agent() {
        let agents = default_agent_catalog();
        let selected = initial_selected_agent_index(&agents, Some("ask"));

        assert_eq!(
            (
                agents[selected].definition_id.as_str(),
                agents[selected].label()
            ),
            ("generalist", "Agent")
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
        let mut effort = ThinkingEffort::Minimal;
        for expected in [
            ThinkingEffort::Low,
            ThinkingEffort::Medium,
            ThinkingEffort::High,
            ThinkingEffort::XHigh,
            ThinkingEffort::Minimal,
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
    fn one_plus_one_golden_inline_viewport_and_scrollback() {
        // The viewport asserts the composer + footer state; the scroll-
        // back assertions cover the conversation content emitted via
        // `transcript::message_lines` / `tool_pill_lines`.
        let mut app = empty_app();
        app.fake_provider_fixture = true;
        let user = RuntimeMessageRow {
            role: "user".into(),
            content: "1+1".into(),
            thinking: None,
            tool_calls: Vec::new(),
        };
        let assistant = RuntimeMessageRow {
            role: "assistant".into(),
            content: "2".into(),
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

        let viewport = render_to_string(&app, 100, 8);
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
