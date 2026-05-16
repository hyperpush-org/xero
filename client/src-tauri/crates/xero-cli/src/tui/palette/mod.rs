//! Command palette: the only entry point to everything that isn't a prompt.
//!
//! Ctrl+P opens [`PaletteState::Browse`] — a filterable, centered list of
//! every command. Pressing Enter dispatches to a per-entry detail view
//! ([`PaletteState::Detail`]) defined in a sibling module. Esc steps back:
//! Detail → Browse → closed.

mod agents;
mod auth;
mod context;
mod events;
mod files;
mod git;
mod new;
mod processes;
mod providers;
mod quit;
mod recovery;
mod register;
mod sessions;
mod settings;
mod skills;
mod usage;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Paragraph, Wrap},
    Frame,
};
use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::{app::App, text_edit, text_edit::TextEdit, theme};

/// A registered palette entry. Maps a typed-id (`"sessions"` or
/// `"provider list"`) to a human title and an action.
pub(crate) struct Command {
    pub id: &'static str,
    pub title: &'static str,
    pub hint: &'static str,
    pub action: CommandAction,
}

#[derive(Clone, Copy)]
pub(crate) enum CommandAction {
    /// Build the detail view for this command — usually by calling a CLI.
    /// Returning `OpenOutcome::Closed` runs the action and dismisses the
    /// palette without ever showing a detail view (used by `quit`,
    /// `register`, `new`).
    Open(fn(&GlobalOptions, &mut App) -> OpenOutcome),
    /// A terminal-safe CLI command surfaced directly in the palette.
    Cli(CliCommandSpec),
}

#[derive(Clone, Copy)]
pub(crate) struct CliCommandSpec {
    pub args: &'static [&'static str],
    pub scope: CommandScope,
    pub mode: CliCommandMode,
}

#[derive(Clone, Copy)]
pub(crate) enum CliCommandMode {
    /// Invoke the CLI command immediately and show the JSON/text result.
    Run,
    /// Do not mutate state from the palette. Show the slash command shape
    /// so the user can run it intentionally from the composer with args.
    Preview,
}

#[derive(Clone, Copy)]
pub(crate) enum CommandScope {
    None,
    ProjectOptional,
    ProjectRequired,
    RepoFallback,
}

#[derive(Debug)]
pub enum OpenOutcome {
    /// Show this detail view inside the palette overlay.
    Detail(DetailState),
    /// Action ran inline; close the palette. Optional status message goes
    /// into `app.status` and gets shown in the footer.
    Closed { status: Option<String> },
    /// Action requested the host to quit.
    Quit,
}

/// Render state for an entry's detail view.
#[derive(Debug)]
pub struct DetailState {
    pub command_id: &'static str,
    pub title: String,
    pub hint: Option<String>,
    pub data: DetailData,
    pub selected: usize,
}

#[derive(Debug)]
pub enum DetailData {
    Rows(Vec<DetailRow>),
    Body(Vec<String>),
    Empty(String),
}

#[derive(Debug, Clone)]
pub struct DetailRow {
    pub title: String,
    pub subtitle: Option<String>,
    /// Opaque payload — entries unpack it when handling Enter.
    pub payload: JsonValue,
}

/// Top-level palette state held by [`App`].
#[derive(Debug)]
pub enum PaletteState {
    Browse(BrowseState),
    Detail(DetailState),
}

#[derive(Debug, Default)]
pub struct BrowseState {
    pub input: String,
    pub input_cursor: usize,
    pub input_desired_column: Option<usize>,
    pub selected: usize,
    /// When `Some`, restrict matches to a single category. Tab cycles
    /// between categories; backspace on empty input clears the filter.
    pub category_filter: Option<Category>,
}

/// Logical grouping that drives the section headers and the category
/// filter chip. Derived from each command's `id` in [`category_for`].
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum Category {
    Session,
    Project,
    Workspace,
    Git,
    Provider,
    Mcp,
    Agent,
    Process,
    Environment,
    Notification,
    Settings,
    Danger,
}

impl Category {
    pub fn label(self) -> &'static str {
        match self {
            Category::Session => "Sessions & Runs",
            Category::Project => "Project",
            Category::Workspace => "Workspace & Files",
            Category::Git => "Git",
            Category::Provider => "Providers & Auth",
            Category::Mcp => "MCP",
            Category::Agent => "Agents & Skills",
            Category::Process => "Processes",
            Category::Environment => "Environment",
            Category::Notification => "Notifications",
            Category::Settings => "Settings & Info",
            Category::Danger => "Danger Zone",
        }
    }

    /// Ordering used when rendering category sections.
    fn order(self) -> u8 {
        match self {
            Category::Session => 0,
            Category::Project => 1,
            Category::Workspace => 2,
            Category::Git => 3,
            Category::Provider => 4,
            Category::Mcp => 5,
            Category::Agent => 6,
            Category::Process => 7,
            Category::Environment => 8,
            Category::Notification => 9,
            Category::Settings => 10,
            Category::Danger => 11,
        }
    }

    fn all() -> &'static [Category] {
        &[
            Category::Session,
            Category::Project,
            Category::Workspace,
            Category::Git,
            Category::Provider,
            Category::Mcp,
            Category::Agent,
            Category::Process,
            Category::Environment,
            Category::Notification,
            Category::Settings,
            Category::Danger,
        ]
    }
}

pub(crate) fn category_for(id: &str) -> Category {
    let root = id.split_whitespace().next().unwrap_or("");
    match root {
        "sessions" | "new" | "session" | "conversation" => Category::Session,
        "register" | "project" | "project-state" => Category::Project,
        "files" | "file" | "workspace" => Category::Workspace,
        "git" | "commit-message" => Category::Git,
        "providers" | "provider" | "auth" | "login" | "logout" | "remote" => Category::Provider,
        "mcp" => Category::Mcp,
        "agents" | "agent" | "agent-definition" | "skills" | "plugins" => Category::Agent,
        "processes" | "process" => Category::Process,
        "environment" | "tool-pack" | "suggest-command" => Category::Environment,
        "notification" => Category::Notification,
        "settings" | "context" | "usage" | "events" | "recovery" => Category::Settings,
        "wipe" | "quit" => Category::Danger,
        _ => Category::Settings,
    }
}

/// Curated commands shown under the "Essentials" header when the input
/// is empty and no category filter is active. Order is intentional.
const ESSENTIALS: &[&str] = &[
    "sessions",
    "new",
    "providers",
    "files",
    "git",
    "agents",
    "context",
    "settings",
];

/// One scored match produced by [`scored_matches`]. Highlight is a pair
/// of (offset, length) into the rendered title — the renderer uses it
/// to draw matched chars in the accent color.
#[derive(Clone, Copy)]
pub(crate) struct MatchHit {
    pub command: &'static Command,
    pub score: i32,
    pub highlight: Option<(usize, usize)>,
}

/// Outcome of handling a key inside the palette overlay.
pub enum KeyResult {
    /// Keep the palette open as-is.
    Continue,
    /// Close the palette and return to the composer.
    Close { status: Option<String> },
    /// Quit the TUI.
    Quit,
}

/// Static command registry. Order is the order shown to the user.
#[allow(dead_code)]
pub(crate) fn commands() -> &'static [Command] {
    COMMANDS
}

static COMMANDS: &[Command] = &[
    Command {
        id: "sessions",
        title: "sessions",
        hint: "switch session",
        action: CommandAction::Open(sessions::open),
    },
    Command {
        id: "new",
        title: "new",
        hint: "new session",
        action: CommandAction::Open(new::open),
    },
    Command {
        id: "files",
        title: "files",
        hint: "browse files",
        action: CommandAction::Open(files::open),
    },
    Command {
        id: "git",
        title: "git",
        hint: "staged / unstaged / commit",
        action: CommandAction::Open(git::open),
    },
    Command {
        id: "processes",
        title: "processes",
        hint: "managed shells",
        action: CommandAction::Open(processes::open),
    },
    Command {
        id: "providers",
        title: "providers",
        hint: "switch model",
        action: CommandAction::Open(providers::open),
    },
    Command {
        id: "auth",
        title: "auth",
        hint: "provider / mcp / remote login",
        action: CommandAction::Open(auth::open),
    },
    Command {
        id: "login",
        title: "login",
        hint: "sign in with GitHub",
        action: CommandAction::Open(super::app::run_login_step),
    },
    Command {
        id: "logout",
        title: "logout",
        hint: "sign out of GitHub",
        action: CommandAction::Open(super::app::run_logout_step),
    },
    Command {
        id: "agents",
        title: "agents",
        hint: "agent definitions",
        action: CommandAction::Open(agents::open),
    },
    Command {
        id: "skills",
        title: "skills",
        hint: "skills & plugins",
        action: CommandAction::Open(skills::open),
    },
    Command {
        id: "context",
        title: "context",
        hint: "todos / plan / memory",
        action: CommandAction::Open(context::open),
    },
    Command {
        id: "recovery",
        title: "recovery",
        hint: "branch / rewind / export",
        action: CommandAction::Open(recovery::open),
    },
    Command {
        id: "usage",
        title: "usage",
        hint: "tokens & cost",
        action: CommandAction::Open(usage::open),
    },
    Command {
        id: "events",
        title: "events",
        hint: "raw event log",
        action: CommandAction::Open(events::open),
    },
    Command {
        id: "settings",
        title: "settings",
        hint: "runtime / browser / behavior",
        action: CommandAction::Open(settings::open),
    },
    Command {
        id: "register",
        title: "register",
        hint: "register this directory as a project",
        action: CommandAction::Open(register::open),
    },
    Command {
        id: "quit",
        title: "quit",
        hint: "exit the TUI",
        action: CommandAction::Open(quit::open),
    },
    Command {
        id: "project list",
        title: "project list",
        hint: "registered projects",
        action: run_cli(&["project", "list"], CommandScope::None),
    },
    Command {
        id: "project import",
        title: "project import",
        hint: "register a repo from disk",
        action: preview_cli(&["project", "import"], CommandScope::None),
    },
    Command {
        id: "project create",
        title: "project create",
        hint: "create and register repo",
        action: preview_cli(&["project", "create"], CommandScope::None),
    },
    Command {
        id: "project remove",
        title: "project remove",
        hint: "remove project registration",
        action: preview_cli(&["project", "remove"], CommandScope::None),
    },
    Command {
        id: "project snapshot",
        title: "project snapshot",
        hint: "current project record",
        action: run_cli(&["project", "snapshot"], CommandScope::ProjectOptional),
    },
    Command {
        id: "project select",
        title: "project select",
        hint: "select active TUI project",
        action: preview_cli(&["project", "select"], CommandScope::None),
    },
    Command {
        id: "session list",
        title: "session list",
        hint: "project sessions",
        action: run_cli(&["session", "list"], CommandScope::ProjectRequired),
    },
    Command {
        id: "session create",
        title: "session create",
        hint: "new project session",
        action: preview_cli(&["session", "create"], CommandScope::ProjectRequired),
    },
    Command {
        id: "session rename",
        title: "session rename",
        hint: "rename session",
        action: preview_cli(&["session", "rename"], CommandScope::ProjectRequired),
    },
    Command {
        id: "session auto-name",
        title: "session auto-name",
        hint: "generate session title",
        action: preview_cli(&["session", "auto-name"], CommandScope::ProjectRequired),
    },
    Command {
        id: "session archive",
        title: "session archive",
        hint: "archive session",
        action: preview_cli(&["session", "archive"], CommandScope::ProjectRequired),
    },
    Command {
        id: "session restore",
        title: "session restore",
        hint: "restore session",
        action: preview_cli(&["session", "restore"], CommandScope::ProjectRequired),
    },
    Command {
        id: "session delete",
        title: "session delete",
        hint: "delete session",
        action: preview_cli(&["session", "delete"], CommandScope::ProjectRequired),
    },
    Command {
        id: "session resume",
        title: "session resume",
        hint: "select session",
        action: preview_cli(&["session", "resume"], CommandScope::ProjectRequired),
    },
    Command {
        id: "session select",
        title: "session select",
        hint: "select session",
        action: preview_cli(&["session", "select"], CommandScope::ProjectRequired),
    },
    Command {
        id: "provider list",
        title: "provider list",
        hint: "models and credential kinds",
        action: run_cli(&["provider", "list"], CommandScope::None),
    },
    Command {
        id: "provider login",
        title: "provider login",
        hint: "auth / API-key profile",
        action: preview_cli(&["provider", "login"], CommandScope::None),
    },
    Command {
        id: "provider remove",
        title: "provider remove",
        hint: "remove auth profile",
        action: preview_cli(&["provider", "remove"], CommandScope::None),
    },
    Command {
        id: "provider logout",
        title: "provider logout",
        hint: "remove auth profile",
        action: preview_cli(&["provider", "logout"], CommandScope::None),
    },
    Command {
        id: "provider doctor",
        title: "provider doctor",
        hint: "auth diagnostics",
        action: run_cli(&["provider", "doctor"], CommandScope::None),
    },
    Command {
        id: "provider preflight",
        title: "provider preflight",
        hint: "model contract check",
        action: run_cli(&["provider", "preflight"], CommandScope::None),
    },
    Command {
        id: "mcp list",
        title: "mcp list",
        hint: "configured MCP servers",
        action: run_cli(&["mcp", "list"], CommandScope::None),
    },
    Command {
        id: "mcp add",
        title: "mcp add",
        hint: "add MCP server",
        action: preview_cli(&["mcp", "add"], CommandScope::None),
    },
    Command {
        id: "mcp login",
        title: "mcp login",
        hint: "MCP auth",
        action: preview_cli(&["mcp", "login"], CommandScope::None),
    },
    Command {
        id: "mcp remove",
        title: "mcp remove",
        hint: "remove MCP server",
        action: preview_cli(&["mcp", "remove"], CommandScope::None),
    },
    Command {
        id: "mcp status",
        title: "mcp status",
        hint: "MCP readiness",
        action: run_cli(&["mcp", "status"], CommandScope::None),
    },
    Command {
        id: "mcp serve",
        title: "mcp serve",
        hint: "stdio MCP server",
        action: preview_cli(&["mcp", "serve"], CommandScope::None),
    },
    Command {
        id: "workspace status",
        title: "workspace status",
        hint: "index freshness",
        action: run_cli(&["workspace", "status"], CommandScope::RepoFallback),
    },
    Command {
        id: "workspace index",
        title: "workspace index",
        hint: "build semantic index",
        action: preview_cli(&["workspace", "index"], CommandScope::RepoFallback),
    },
    Command {
        id: "workspace query",
        title: "workspace query",
        hint: "semantic search",
        action: preview_cli(&["workspace", "query"], CommandScope::RepoFallback),
    },
    Command {
        id: "workspace explain",
        title: "workspace explain",
        hint: "retrieval signals",
        action: preview_cli(&["workspace", "explain"], CommandScope::RepoFallback),
    },
    Command {
        id: "workspace reset",
        title: "workspace reset",
        hint: "clear workspace index",
        action: preview_cli(&["workspace", "reset"], CommandScope::RepoFallback),
    },
    Command {
        id: "file list",
        title: "file list",
        hint: "project file tree",
        action: run_cli(&["file", "list"], CommandScope::RepoFallback),
    },
    Command {
        id: "file read",
        title: "file read",
        hint: "read file",
        action: preview_cli(&["file", "read"], CommandScope::RepoFallback),
    },
    Command {
        id: "file write",
        title: "file write",
        hint: "write file through registry",
        action: preview_cli(&["file", "write"], CommandScope::RepoFallback),
    },
    Command {
        id: "file patch",
        title: "file patch",
        hint: "apply unified diff",
        action: preview_cli(&["file", "patch"], CommandScope::RepoFallback),
    },
    Command {
        id: "file delete",
        title: "file delete",
        hint: "delete file",
        action: preview_cli(&["file", "delete"], CommandScope::RepoFallback),
    },
    Command {
        id: "file move",
        title: "file move",
        hint: "rename/move file",
        action: preview_cli(&["file", "move"], CommandScope::RepoFallback),
    },
    Command {
        id: "file replace",
        title: "file replace",
        hint: "search and replace",
        action: preview_cli(&["file", "replace"], CommandScope::RepoFallback),
    },
    Command {
        id: "file tools",
        title: "file tools",
        hint: "registry file tools",
        action: run_cli(&["file", "tools"], CommandScope::RepoFallback),
    },
    Command {
        id: "git status",
        title: "git status",
        hint: "working tree status",
        action: run_cli(&["git", "status"], CommandScope::RepoFallback),
    },
    Command {
        id: "git diff",
        title: "git diff",
        hint: "repo diff",
        action: run_cli(&["git", "diff"], CommandScope::RepoFallback),
    },
    Command {
        id: "git stage",
        title: "git stage",
        hint: "stage paths",
        action: preview_cli(&["git", "stage"], CommandScope::RepoFallback),
    },
    Command {
        id: "git unstage",
        title: "git unstage",
        hint: "unstage paths",
        action: preview_cli(&["git", "unstage"], CommandScope::RepoFallback),
    },
    Command {
        id: "git discard",
        title: "git discard",
        hint: "discard worktree changes",
        action: preview_cli(&["git", "discard"], CommandScope::RepoFallback),
    },
    Command {
        id: "git commit",
        title: "git commit",
        hint: "commit staged changes",
        action: preview_cli(&["git", "commit"], CommandScope::RepoFallback),
    },
    Command {
        id: "git fetch",
        title: "git fetch",
        hint: "fetch remote refs",
        action: preview_cli(&["git", "fetch"], CommandScope::RepoFallback),
    },
    Command {
        id: "git pull",
        title: "git pull",
        hint: "ff-only pull",
        action: preview_cli(&["git", "pull"], CommandScope::RepoFallback),
    },
    Command {
        id: "git push",
        title: "git push",
        hint: "push current branch",
        action: preview_cli(&["git", "push"], CommandScope::RepoFallback),
    },
    Command {
        id: "agent-definition list",
        title: "agent-definition list",
        hint: "agent catalog",
        action: run_cli(&["agent-definition", "list"], CommandScope::ProjectRequired),
    },
    Command {
        id: "agent-definition show",
        title: "agent-definition show",
        hint: "definition snapshot",
        action: preview_cli(&["agent-definition", "show"], CommandScope::ProjectRequired),
    },
    Command {
        id: "agent-definition versions",
        title: "agent-definition versions",
        hint: "definition history",
        action: preview_cli(
            &["agent-definition", "versions"],
            CommandScope::ProjectRequired,
        ),
    },
    Command {
        id: "agent-definition diff",
        title: "agent-definition diff",
        hint: "version diff",
        action: preview_cli(&["agent-definition", "diff"], CommandScope::ProjectRequired),
    },
    Command {
        id: "agent-definition archive",
        title: "agent-definition archive",
        hint: "archive definition",
        action: preview_cli(
            &["agent-definition", "archive"],
            CommandScope::ProjectRequired,
        ),
    },
    Command {
        id: "skills list",
        title: "skills list",
        hint: "installed skills",
        action: run_cli(&["skills", "list"], CommandScope::ProjectOptional),
    },
    Command {
        id: "skills enable",
        title: "skills enable",
        hint: "enable skill",
        action: preview_cli(&["skills", "enable"], CommandScope::ProjectOptional),
    },
    Command {
        id: "skills disable",
        title: "skills disable",
        hint: "disable skill",
        action: preview_cli(&["skills", "disable"], CommandScope::ProjectOptional),
    },
    Command {
        id: "skills remove",
        title: "skills remove",
        hint: "remove skill",
        action: preview_cli(&["skills", "remove"], CommandScope::ProjectOptional),
    },
    Command {
        id: "skills plugins",
        title: "skills plugins",
        hint: "plugins via skills command",
        action: run_cli(&["skills", "plugins"], CommandScope::ProjectOptional),
    },
    Command {
        id: "plugins list",
        title: "plugins list",
        hint: "installed plugins",
        action: run_cli(&["plugins", "list"], CommandScope::ProjectOptional),
    },
    Command {
        id: "plugins enable",
        title: "plugins enable",
        hint: "enable plugin",
        action: preview_cli(&["plugins", "enable"], CommandScope::ProjectOptional),
    },
    Command {
        id: "plugins disable",
        title: "plugins disable",
        hint: "disable plugin",
        action: preview_cli(&["plugins", "disable"], CommandScope::ProjectOptional),
    },
    Command {
        id: "plugins remove",
        title: "plugins remove",
        hint: "remove plugin",
        action: preview_cli(&["plugins", "remove"], CommandScope::ProjectOptional),
    },
    Command {
        id: "notification routes",
        title: "notification routes",
        hint: "approval routes",
        action: run_cli(&["notification", "routes"], CommandScope::ProjectRequired),
    },
    Command {
        id: "notification upsert-route",
        title: "notification upsert-route",
        hint: "add approval route",
        action: preview_cli(
            &["notification", "upsert-route"],
            CommandScope::ProjectRequired,
        ),
    },
    Command {
        id: "notification remove-route",
        title: "notification remove-route",
        hint: "remove approval route",
        action: preview_cli(
            &["notification", "remove-route"],
            CommandScope::ProjectRequired,
        ),
    },
    Command {
        id: "notification dispatches",
        title: "notification dispatches",
        hint: "approval dispatch log",
        action: run_cli(
            &["notification", "dispatches"],
            CommandScope::ProjectRequired,
        ),
    },
    Command {
        id: "notification replies",
        title: "notification replies",
        hint: "approval replies",
        action: run_cli(&["notification", "replies"], CommandScope::ProjectRequired),
    },
    Command {
        id: "environment status",
        title: "environment status",
        hint: "local environment health",
        action: run_cli(&["environment", "status"], CommandScope::None),
    },
    Command {
        id: "environment profile",
        title: "environment profile",
        hint: "discovered tools",
        action: run_cli(&["environment", "profile"], CommandScope::None),
    },
    Command {
        id: "environment user-tools",
        title: "environment user-tools",
        hint: "saved custom tools",
        action: run_cli(&["environment", "user-tools"], CommandScope::None),
    },
    Command {
        id: "environment save-tool",
        title: "environment save-tool",
        hint: "save custom tool",
        action: preview_cli(&["environment", "save-tool"], CommandScope::None),
    },
    Command {
        id: "environment remove-tool",
        title: "environment remove-tool",
        hint: "remove custom tool",
        action: preview_cli(&["environment", "remove-tool"], CommandScope::None),
    },
    Command {
        id: "settings agent-tooling",
        title: "settings agent-tooling",
        hint: "agent tool behavior",
        action: run_cli(&["settings", "agent-tooling"], CommandScope::None),
    },
    Command {
        id: "settings browser-control",
        title: "settings browser-control",
        hint: "browser tool preference",
        action: run_cli(&["settings", "browser-control"], CommandScope::None),
    },
    Command {
        id: "settings soul",
        title: "settings soul",
        hint: "behavior preset",
        action: run_cli(&["settings", "soul"], CommandScope::None),
    },
    Command {
        id: "usage summary",
        title: "usage summary",
        hint: "tokens and cost",
        action: run_cli(&["usage", "summary"], CommandScope::ProjectRequired),
    },
    Command {
        id: "project-state list",
        title: "project-state list",
        hint: "state backups",
        action: run_cli(&["project-state", "list"], CommandScope::ProjectRequired),
    },
    Command {
        id: "project-state backup",
        title: "project-state backup",
        hint: "create backup",
        action: preview_cli(&["project-state", "backup"], CommandScope::ProjectRequired),
    },
    Command {
        id: "project-state restore",
        title: "project-state restore",
        hint: "restore backup",
        action: preview_cli(&["project-state", "restore"], CommandScope::ProjectRequired),
    },
    Command {
        id: "project-state repair",
        title: "project-state repair",
        hint: "repair state database",
        action: preview_cli(&["project-state", "repair"], CommandScope::ProjectRequired),
    },
    Command {
        id: "tool-pack list",
        title: "tool-pack list",
        hint: "domain packs",
        action: run_cli(&["tool-pack", "list"], CommandScope::None),
    },
    Command {
        id: "tool-pack doctor",
        title: "tool-pack doctor",
        hint: "domain prerequisites",
        action: run_cli(&["tool-pack", "doctor"], CommandScope::None),
    },
    Command {
        id: "conversation list",
        title: "conversation list",
        hint: "runs for recovery",
        action: run_cli(&["conversation", "list"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation dump",
        title: "conversation dump",
        hint: "raw run dump",
        action: preview_cli(&["conversation", "dump"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation support-bundle",
        title: "conversation support-bundle",
        hint: "support bundle",
        action: preview_cli(
            &["conversation", "support-bundle"],
            CommandScope::ProjectRequired,
        ),
    },
    Command {
        id: "conversation continue",
        title: "conversation continue",
        hint: "continue run",
        action: preview_cli(&["conversation", "continue"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation answer",
        title: "conversation answer",
        hint: "answer user input",
        action: preview_cli(&["conversation", "answer"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation show",
        title: "conversation show",
        hint: "show run snapshot",
        action: preview_cli(&["conversation", "show"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation search",
        title: "conversation search",
        hint: "search transcripts",
        action: preview_cli(&["conversation", "search"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation export",
        title: "conversation export",
        hint: "export transcript",
        action: preview_cli(&["conversation", "export"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation compact",
        title: "conversation compact",
        hint: "compact context",
        action: preview_cli(&["conversation", "compact"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation approve",
        title: "conversation approve",
        hint: "approve action",
        action: preview_cli(&["conversation", "approve"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation deny",
        title: "conversation deny",
        hint: "deny action",
        action: preview_cli(&["conversation", "deny"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation cancel",
        title: "conversation cancel",
        hint: "cancel run",
        action: preview_cli(&["conversation", "cancel"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation resume",
        title: "conversation resume",
        hint: "resume run",
        action: preview_cli(&["conversation", "resume"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation retry",
        title: "conversation retry",
        hint: "retry run",
        action: preview_cli(&["conversation", "retry"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation branch",
        title: "conversation branch",
        hint: "branch session",
        action: preview_cli(&["conversation", "branch"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation clone",
        title: "conversation clone",
        hint: "clone session",
        action: preview_cli(&["conversation", "clone"], CommandScope::ProjectRequired),
    },
    Command {
        id: "conversation stats",
        title: "conversation stats",
        hint: "run statistics",
        action: preview_cli(&["conversation", "stats"], CommandScope::ProjectRequired),
    },
    Command {
        id: "process targets",
        title: "process targets",
        hint: "start targets",
        action: run_cli(&["process", "targets"], CommandScope::ProjectRequired),
    },
    Command {
        id: "process add-target",
        title: "process add-target",
        hint: "save start target",
        action: preview_cli(&["process", "add-target"], CommandScope::ProjectRequired),
    },
    Command {
        id: "process remove-target",
        title: "process remove-target",
        hint: "remove start target",
        action: preview_cli(&["process", "remove-target"], CommandScope::ProjectRequired),
    },
    Command {
        id: "process start",
        title: "process start",
        hint: "start target",
        action: preview_cli(&["process", "start"], CommandScope::ProjectRequired),
    },
    Command {
        id: "process list",
        title: "process list",
        hint: "running process sessions",
        action: run_cli(&["process", "list"], CommandScope::ProjectRequired),
    },
    Command {
        id: "process status",
        title: "process status",
        hint: "process details",
        action: preview_cli(&["process", "status"], CommandScope::ProjectRequired),
    },
    Command {
        id: "process tail",
        title: "process tail",
        hint: "process output",
        action: preview_cli(&["process", "tail"], CommandScope::ProjectRequired),
    },
    Command {
        id: "process stop",
        title: "process stop",
        hint: "stop process",
        action: preview_cli(&["process", "stop"], CommandScope::ProjectRequired),
    },
    Command {
        id: "remote login",
        title: "remote login",
        hint: "GitHub auth",
        action: preview_cli(&["remote", "login"], CommandScope::None),
    },
    Command {
        id: "remote logout",
        title: "remote logout",
        hint: "clear GitHub auth",
        action: preview_cli(&["remote", "logout"], CommandScope::None),
    },
    Command {
        id: "remote devices",
        title: "remote devices",
        hint: "remote auth devices",
        action: run_cli(&["remote", "devices"], CommandScope::None),
    },
    Command {
        id: "remote connect",
        title: "remote connect",
        hint: "connect remote session",
        action: preview_cli(&["remote", "connect"], CommandScope::None),
    },
    Command {
        id: "remote visibility",
        title: "remote visibility",
        hint: "session visibility",
        action: preview_cli(&["remote", "visibility"], CommandScope::ProjectRequired),
    },
    Command {
        id: "agent host",
        title: "agent host",
        hint: "external CLI agent",
        action: preview_cli(&["agent", "host"], CommandScope::RepoFallback),
    },
    Command {
        id: "wipe project",
        title: "wipe project",
        hint: "delete project app-data",
        action: preview_cli(&["wipe", "project"], CommandScope::None),
    },
    Command {
        id: "wipe all",
        title: "wipe all",
        hint: "delete all app-data",
        action: preview_cli(&["wipe", "all"], CommandScope::None),
    },
    Command {
        id: "commit-message",
        title: "commit-message",
        hint: "generate commit message",
        action: preview_cli(&["commit-message"], CommandScope::RepoFallback),
    },
    Command {
        id: "suggest-command",
        title: "suggest-command",
        hint: "suggest a terminal command",
        action: preview_cli(&["suggest-command"], CommandScope::RepoFallback),
    },
];

const fn run_cli(args: &'static [&'static str], scope: CommandScope) -> CommandAction {
    CommandAction::Cli(CliCommandSpec {
        args,
        scope,
        mode: CliCommandMode::Run,
    })
}

const fn preview_cli(args: &'static [&'static str], scope: CommandScope) -> CommandAction {
    CommandAction::Cli(CliCommandSpec {
        args,
        scope,
        mode: CliCommandMode::Preview,
    })
}

/// Open the palette (used by Ctrl+P).
pub fn open() -> PaletteState {
    PaletteState::Browse(BrowseState::default())
}

/// Backwards-compatible filter used by tests and the slash module.
#[allow(dead_code)]
pub(crate) fn filtered(input: &str) -> Vec<&'static Command> {
    scored_matches(input, None)
        .into_iter()
        .map(|hit| hit.command)
        .collect()
}

/// Score the registry against `input` and an optional category filter.
/// Returns matches ordered by score desc, then by category, then by id.
pub(crate) fn scored_matches(input: &str, category: Option<Category>) -> Vec<MatchHit> {
    let trimmed = input.trim();
    let mut hits: Vec<MatchHit> = COMMANDS
        .iter()
        .filter(|cmd| category.is_none_or(|c| category_for(cmd.id) == c))
        .filter_map(|cmd| score_command(cmd, trimmed))
        .collect();
    hits.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| {
                category_for(a.command.id)
                    .order()
                    .cmp(&category_for(b.command.id).order())
            })
            .then_with(|| a.command.id.cmp(b.command.id))
    });
    hits
}

fn score_command(cmd: &'static Command, needle: &str) -> Option<MatchHit> {
    if needle.is_empty() {
        return Some(MatchHit {
            command: cmd,
            score: 0,
            highlight: None,
        });
    }
    let needle_lo = needle.to_lowercase();
    let id_lo = cmd.id.to_lowercase();
    let title_lo = cmd.title.to_lowercase();
    let hint_lo = cmd.hint.to_lowercase();

    // Tier 1 — exact id match.
    if id_lo == needle_lo {
        return Some(MatchHit {
            command: cmd,
            score: 10_000,
            highlight: Some((0, cmd.title.len())),
        });
    }
    // Tier 2 — id or title prefix.
    if let Some(stripped) = id_lo.strip_prefix(&needle_lo) {
        let hl_len = cmd
            .title
            .len()
            .saturating_sub(cmd.id.len() - needle_lo.len());
        let _ = stripped;
        return Some(MatchHit {
            command: cmd,
            score: 8_000 - id_lo.len() as i32,
            highlight: Some((0, hl_len.min(cmd.title.len()))),
        });
    }
    if title_lo.starts_with(&needle_lo) {
        return Some(MatchHit {
            command: cmd,
            score: 7_500 - title_lo.len() as i32,
            highlight: Some((0, needle_lo.len().min(cmd.title.len()))),
        });
    }
    // Tier 3 — acronym (e.g. "pl" → "project list").
    if let Some(score) = acronym_score(&id_lo, &needle_lo) {
        return Some(MatchHit {
            command: cmd,
            score: 6_000 + score,
            highlight: None,
        });
    }
    // Tier 4 — substring matches.
    if let Some(pos) = id_lo.find(&needle_lo) {
        let hl = highlight_in_title(cmd, &id_lo, pos, needle_lo.len());
        return Some(MatchHit {
            command: cmd,
            score: 4_000 - pos as i32,
            highlight: hl,
        });
    }
    if let Some(pos) = title_lo.find(&needle_lo) {
        return Some(MatchHit {
            command: cmd,
            score: 3_000 - pos as i32,
            highlight: Some((pos, needle_lo.len())),
        });
    }
    if hint_lo.contains(&needle_lo) {
        return Some(MatchHit {
            command: cmd,
            score: 1_500,
            highlight: None,
        });
    }
    // Tier 5 — loose fuzzy (every char of needle appears in id in order).
    fuzzy_score(&id_lo, &needle_lo).map(|score| MatchHit {
        command: cmd,
        score,
        highlight: None,
    })
}

fn acronym_score(id: &str, needle: &str) -> Option<i32> {
    let words: Vec<&str> = id
        .split(|c: char| c.is_whitespace() || c == '-' || c == '_')
        .filter(|w| !w.is_empty())
        .collect();
    if needle.chars().count() > words.len() || needle.is_empty() {
        return None;
    }
    for (i, ch) in needle.chars().enumerate() {
        if !words[i].starts_with(ch) {
            return None;
        }
    }
    Some(200 - (words.len() as i32 - needle.chars().count() as i32))
}

fn fuzzy_score(haystack: &str, needle: &str) -> Option<i32> {
    let mut hi = 0;
    let mut last = None;
    let mut gap_penalty: i32 = 0;
    let hay: Vec<char> = haystack.chars().collect();
    let nd: Vec<char> = needle.chars().collect();
    for ch in nd.iter().copied() {
        while hi < hay.len() && hay[hi] != ch {
            hi += 1;
        }
        if hi >= hay.len() {
            return None;
        }
        if let Some(prev) = last {
            gap_penalty += (hi - prev - 1) as i32;
        }
        last = Some(hi);
        hi += 1;
    }
    Some(500 - gap_penalty.min(500))
}

fn highlight_in_title(
    cmd: &Command,
    id_lo: &str,
    pos_in_id: usize,
    len: usize,
) -> Option<(usize, usize)> {
    let title_lo = cmd.title.to_lowercase();
    if title_lo == id_lo {
        return Some((pos_in_id, len));
    }
    title_lo
        .find(&id_lo[pos_in_id..pos_in_id + len])
        .map(|p| (p, len))
}

/// Handle a key while the palette is open. The caller owns mutation of
/// [`App::palette`]; this returns the next state via [`KeyResult`].
pub fn handle_key(app: &mut App, key: KeyEvent, globals: &GlobalOptions) -> KeyResult {
    // We temporarily take the state out of App so we can match by value
    // and re-insert the new state in one shot.
    let Some(state) = app.palette.take() else {
        return KeyResult::Close { status: None };
    };
    match state {
        PaletteState::Browse(browse) => browse_key(app, browse, key, globals),
        PaletteState::Detail(detail) => detail_key(app, detail, key, globals),
    }
}

fn browse_key(
    app: &mut App,
    mut browse: BrowseState,
    key: KeyEvent,
    globals: &GlobalOptions,
) -> KeyResult {
    let matches = scored_matches(&browse.input, browse.category_filter);
    match key.code {
        KeyCode::Esc => {
            if browse.category_filter.is_some() {
                browse.category_filter = None;
                browse.selected = 0;
                app.palette = Some(PaletteState::Browse(browse));
                KeyResult::Continue
            } else {
                KeyResult::Close { status: None }
            }
        }
        KeyCode::Up => {
            move_visual_selection(&mut browse, &matches, -1);
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::Down => {
            move_visual_selection(&mut browse, &matches, 1);
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::PageUp => {
            move_visual_selection(&mut browse, &matches, -8);
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::PageDown => {
            move_visual_selection(&mut browse, &matches, 8);
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::Home => {
            select_visual_endpoint(&mut browse, &matches, false);
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::End => {
            select_visual_endpoint(&mut browse, &matches, true);
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::Tab => {
            browse.category_filter = cycle_category(browse.category_filter, false);
            browse.selected = 0;
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::BackTab => {
            browse.category_filter = cycle_category(browse.category_filter, true);
            browse.selected = 0;
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::Enter => {
            let Some(hit) = matches.get(browse.selected) else {
                app.palette = Some(PaletteState::Browse(browse));
                return KeyResult::Continue;
            };
            activate_command(hit.command, app, globals)
        }
        _ => {
            handle_browse_text_edit(&mut browse, key);
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
    }
}

fn move_visual_selection(browse: &mut BrowseState, matches: &[MatchHit], offset: isize) {
    let order = visual_command_indices(browse, matches);
    if order.is_empty() {
        browse.selected = 0;
        return;
    }

    let current = order
        .iter()
        .position(|match_index| *match_index == browse.selected)
        .unwrap_or(0);
    let next = if offset.is_negative() {
        current.saturating_sub(offset.unsigned_abs())
    } else {
        (current + offset as usize).min(order.len() - 1)
    };
    browse.selected = order[next];
}

fn select_visual_endpoint(browse: &mut BrowseState, matches: &[MatchHit], last: bool) {
    let order = visual_command_indices(browse, matches);
    let Some(selected) = (if last { order.last() } else { order.first() }) else {
        browse.selected = 0;
        return;
    };
    browse.selected = *selected;
}

fn visual_command_indices(browse: &BrowseState, matches: &[MatchHit]) -> Vec<usize> {
    build_list_rows(browse, matches)
        .into_iter()
        .filter_map(|row| match row {
            BrowseRow::Command { match_index, .. } => Some(match_index),
            BrowseRow::Spacer | BrowseRow::Header { .. } => None,
        })
        .collect()
}

fn cycle_category(current: Option<Category>, backward: bool) -> Option<Category> {
    let all = Category::all();
    let idx = current
        .and_then(|c| all.iter().position(|other| *other == c))
        .map(|p| p as isize);
    let next_idx = match (idx, backward) {
        (None, false) => 0,
        (None, true) => all.len() as isize - 1,
        (Some(p), false) => p + 1,
        (Some(p), true) => p - 1,
    };
    if next_idx < 0 || next_idx >= all.len() as isize {
        None
    } else {
        Some(all[next_idx as usize])
    }
}

fn handle_browse_text_edit(browse: &mut BrowseState, key: KeyEvent) {
    let edit = text_edit::edit_for_key(key);
    let outcome = text_edit::apply_edit_at_cursor(
        &mut browse.input,
        &mut browse.input_cursor,
        &mut browse.input_desired_column,
        edit,
    );
    if !outcome.changed {
        return;
    }
    if outcome.text_changed {
        match edit {
            TextEdit::Insert(_)
            | TextEdit::Backspace
            | TextEdit::Delete
            | TextEdit::DeletePreviousWord
            | TextEdit::DeleteToLineStart => browse.selected = 0,
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
    }
}

fn detail_key(
    app: &mut App,
    mut detail: DetailState,
    key: KeyEvent,
    globals: &GlobalOptions,
) -> KeyResult {
    // Esc always returns to browse.
    if matches!(key.code, KeyCode::Esc) {
        app.palette = Some(PaletteState::Browse(BrowseState::default()));
        return KeyResult::Continue;
    }
    // Up/Down navigate detail rows uniformly.
    if let DetailData::Rows(ref rows) = detail.data {
        match key.code {
            KeyCode::Up => {
                detail.selected = detail.selected.saturating_sub(1);
                app.palette = Some(PaletteState::Detail(detail));
                return KeyResult::Continue;
            }
            KeyCode::Down => {
                if !rows.is_empty() {
                    detail.selected = (detail.selected + 1).min(rows.len() - 1);
                }
                app.palette = Some(PaletteState::Detail(detail));
                return KeyResult::Continue;
            }
            _ => {}
        }
    }
    // Delegate Enter (and any entry-specific keys) to the entry's handler.
    let command_id = detail.command_id;
    let outcome = dispatch_detail_key(command_id, app, &mut detail, key, globals);
    match outcome {
        DetailOutcome::Stay => {
            app.palette = Some(PaletteState::Detail(detail));
            KeyResult::Continue
        }
        DetailOutcome::BackToBrowse => {
            app.palette = Some(PaletteState::Browse(BrowseState::default()));
            KeyResult::Continue
        }
        DetailOutcome::Close { status } => KeyResult::Close { status },
        DetailOutcome::Quit => KeyResult::Quit,
    }
}

/// Per-entry response to a key event inside a detail view.
#[allow(dead_code)]
pub enum DetailOutcome {
    Stay,
    BackToBrowse,
    Close { status: Option<String> },
    Quit,
}

fn dispatch_detail_key(
    id: &str,
    app: &mut App,
    detail: &mut DetailState,
    key: KeyEvent,
    globals: &GlobalOptions,
) -> DetailOutcome {
    match id {
        "sessions" => sessions::handle_key(app, detail, key, globals),
        "providers" => providers::handle_key(app, detail, key, globals),
        "auth" => auth::handle_key(app, detail, key, globals),
        "files" => files::handle_key(app, detail, key, globals),
        "git" => git::handle_key(app, detail, key, globals),
        "processes" => processes::handle_key(app, detail, key, globals),
        "agents" => agents::handle_key(app, detail, key, globals),
        "skills" => skills::handle_key(app, detail, key, globals),
        "context" => context::handle_key(app, detail, key, globals),
        "recovery" => recovery::handle_key(app, detail, key, globals),
        "usage" => usage::handle_key(app, detail, key, globals),
        "events" => events::handle_key(app, detail, key, globals),
        "settings" => settings::handle_key(app, detail, key, globals),
        // `new`, `register`, `quit` never produce a detail view, so any key
        // here is a soft-close.
        _ => DetailOutcome::BackToBrowse,
    }
}

pub(crate) fn find_command(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.id == id)
}

pub(crate) fn activate_command_by_id(
    id: &str,
    app: &mut App,
    globals: &GlobalOptions,
) -> Option<KeyResult> {
    find_command(id).map(|command| activate_command(command, app, globals))
}

pub(crate) fn activate_command(
    command: &Command,
    app: &mut App,
    globals: &GlobalOptions,
) -> KeyResult {
    let outcome = match command.action {
        CommandAction::Open(open) => open(globals, app),
        CommandAction::Cli(spec) => {
            cli_command_outcome(command.id, command.title, spec, globals, app)
        }
    };
    match outcome {
        OpenOutcome::Detail(detail) => {
            app.palette = Some(PaletteState::Detail(detail));
            KeyResult::Continue
        }
        OpenOutcome::Closed { status } => KeyResult::Close { status },
        OpenOutcome::Quit => KeyResult::Quit,
    }
}

pub(crate) fn cli_command_outcome(
    command_id: &'static str,
    title: impl Into<String>,
    spec: CliCommandSpec,
    globals: &GlobalOptions,
    app: &App,
) -> OpenOutcome {
    let title = title.into();
    if matches!(spec.mode, CliCommandMode::Preview) {
        return body_detail(
            command_id,
            title,
            Some("type slash command in composer to run"),
            preview_lines(spec.args),
        );
    }

    let args = match scoped_args(spec.args, spec.scope, app) {
        Ok(args) => args,
        Err(message) => {
            return empty_detail(command_id, title, message);
        }
    };
    let borrowed = args.iter().map(String::as_str).collect::<Vec<_>>();
    match super::app::invoke_json(globals, &borrowed) {
        Ok(value) => body_detail(
            command_id,
            title,
            Some("esc back   slash commands accept args"),
            detail_lines_for_json(&value),
        ),
        Err(error) => error_detail(command_id, title, error),
    }
}

pub(crate) fn detail_lines_for_json(value: &JsonValue) -> Vec<String> {
    if let Some(text) = value
        .get("output")
        .and_then(JsonValue::as_str)
        .or_else(|| value.get("text").and_then(JsonValue::as_str))
        .filter(|text| !text.trim().is_empty())
    {
        return text.lines().map(str::to_owned).collect();
    }
    match serde_json::to_string_pretty(value) {
        Ok(text) => text.lines().map(str::to_owned).collect(),
        Err(_) => vec!["<unserializable command payload>".to_owned()],
    }
}

pub(crate) fn scoped_args(
    base: &[&str],
    scope: CommandScope,
    app: &App,
) -> Result<Vec<String>, String> {
    let mut args = base.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let has_project = has_option(&args, "--project-id");
    let has_repo = has_option(&args, "--repo");
    match scope {
        CommandScope::None => {}
        CommandScope::ProjectOptional => {
            if !has_project {
                if let Some(project_id) = app.project.project_id.as_deref() {
                    args.push("--project-id".into());
                    args.push(project_id.to_owned());
                }
            }
        }
        CommandScope::ProjectRequired => {
            if !has_project {
                let Some(project_id) = app.project.project_id.as_deref() else {
                    return Err(
                        "No project bound. Use `register` first or pass --project-id from a slash command."
                            .to_owned(),
                    );
                };
                args.push("--project-id".into());
                args.push(project_id.to_owned());
            }
        }
        CommandScope::RepoFallback => {
            if !has_project && !has_repo {
                if let Some(project_id) = app.project.project_id.as_deref() {
                    args.push("--project-id".into());
                    args.push(project_id.to_owned());
                } else if let Some(root) = app.project.root.to_str() {
                    args.push("--repo".into());
                    args.push(root.to_owned());
                }
            }
        }
    }
    Ok(args)
}

fn preview_lines(args: &[&str]) -> Vec<String> {
    let command = args.join(" ");
    vec![
        "Run this from the composer with any required arguments:".to_owned(),
        format!("/{command}"),
        String::new(),
        "Read-only commands run directly from this palette. Commands that need arguments or mutate state are guarded here and run only when typed explicitly."
            .to_owned(),
    ]
}

fn has_option(args: &[String], option: &str) -> bool {
    let prefix = format!("{option}=");
    args.iter()
        .any(|arg| arg == option || arg.starts_with(prefix.as_str()))
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Empty rows inserted above and below the palette content so text doesn't
/// hug the elevated surface's top/bottom edges — mirrors the composer.
const VERTICAL_PAD_ROWS: u16 = 1;
const PALETTE_LEFT_GUTTER: &str = "   ";
const RIGHT_LABEL_PAD: usize = 2;

/// Preferred overlay size for the current palette state. The caller centers
/// a rect of this size inside the viewport. The returned dimensions include
/// the 1-cell decorative halo on every side.
pub fn desired_size(app: &App, viewport: Rect) -> (u16, u16) {
    const MIN_WIDTH: u16 = 56;
    const MAX_WIDTH: u16 = 96;
    let width = viewport.width.saturating_mul(60) / 100;
    let width = width.clamp(MIN_WIDTH.min(viewport.width), MAX_WIDTH);

    let max_height = viewport.height.saturating_mul(85) / 100;
    let preferred_height = match app.palette.as_ref() {
        Some(PaletteState::Browse(_)) => {
            // Browse view: take as much vertical space as we can comfortably
            // afford so longer lists need less scrolling. Floor at 22 rows
            // so even short terminals keep the list, strip, and footer.
            max_height.max(22)
        }
        Some(PaletteState::Detail(detail)) => {
            let body = match &detail.data {
                DetailData::Rows(rows) => rows
                    .iter()
                    .map(|row| if row.subtitle.is_some() { 2 } else { 1 })
                    .sum::<usize>(),
                DetailData::Body(lines) => lines.len(),
                DetailData::Empty(_) => 1,
            }
            .max(1) as u16;
            body + 6
        }
        None => 0,
    };
    let height = preferred_height.min(max_height).max(10);
    (width, height)
}

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(state) = app.palette.as_ref() else {
        return;
    };
    if area.width <= 2 || area.height <= 2 {
        return;
    }
    // Thin gold-rounded border around the overlay surface. Using a Block
    // (rather than painting a halo rectangle) draws actual box-drawing
    // characters in the accent color — same gold as the inline ▎ stripe.
    let bg = theme::composer_bg_color();
    let card = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .style(Style::default().bg(bg))
        .border_style(Style::default().fg(theme::ACCENT).bg(bg));
    let surface_area = card.inner(area);
    frame.render_widget(card, area);

    // Carve out a top-padded inner rect for the actual content so text
    // doesn't crowd the surface's top/bottom edges.
    let inner = inner_rect(surface_area);
    match state {
        PaletteState::Browse(browse) => render_browse(frame, inner, app, browse),
        PaletteState::Detail(detail) => render_detail(frame, inner, detail),
    }
}

fn inner_rect(surface_area: Rect) -> Rect {
    let pad = VERTICAL_PAD_ROWS.min(surface_area.height.saturating_sub(1));
    Rect {
        x: surface_area.x,
        y: surface_area.y + pad,
        width: surface_area.width,
        height: surface_area.height.saturating_sub(2 * pad),
    }
}

fn render_browse(frame: &mut Frame<'_>, area: Rect, app: &App, browse: &BrowseState) {
    let bg = theme::composer_bg_color();
    let matches = scored_matches(&browse.input, browse.category_filter);
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Length(1), // input
            Constraint::Length(1), // spacer
            Constraint::Min(3),    // command list
            Constraint::Length(1), // spacer above footer
            Constraint::Length(1), // footer hints
        ])
        .split(area);

    render_palette_title_bar(frame, chunks[0], app, browse, bg);
    render_palette_input(frame, chunks[1], browse, bg);
    frame.render_widget(blank_paragraph(bg), chunks[2]);
    render_palette_list(frame, chunks[3], browse, &matches, bg);
    frame.render_widget(blank_paragraph(bg), chunks[4]);
    render_palette_footer(frame, chunks[5], browse, bg);
}

fn render_palette_title_bar(
    frame: &mut Frame<'_>,
    area: Rect,
    _app: &App,
    browse: &BrowseState,
    bg: Color,
) {
    let mut spans = vec![
        Span::styled(PALETTE_LEFT_GUTTER, Style::default().bg(bg)),
        Span::styled(
            "Command Palette",
            theme::accent().bg(bg).add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some(category) = browse.category_filter {
        spans.push(Span::styled("  ", theme::dim().bg(bg)));
        spans.push(Span::styled(" ", theme::fg().bg(theme::ACCENT)));
        spans.push(Span::styled(
            format!(" {} ", category.label()),
            Style::default().fg(theme::BG).bg(theme::ACCENT),
        ));
        spans.push(Span::styled(" ", theme::fg().bg(theme::ACCENT)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

fn render_palette_input(frame: &mut Frame<'_>, area: Rect, browse: &BrowseState, bg: Color) {
    let mut spans = vec![
        Span::styled(PALETTE_LEFT_GUTTER, Style::default().bg(bg)),
        Span::styled("❯ ", theme::dim().bg(bg)),
    ];
    if browse.input.is_empty() {
        spans.push(Span::styled("Search commands…", theme::dim().bg(bg)));
    } else {
        let cursor = text_edit::clamped_cursor(&browse.input, browse.input_cursor);
        let text_style = theme::fg().bg(bg).add_modifier(Modifier::BOLD);
        if cursor > 0 {
            spans.push(Span::styled(browse.input[..cursor].to_string(), text_style));
        }
        if cursor < browse.input.len() {
            let cursor_char_end = browse.input[cursor..]
                .char_indices()
                .nth(1)
                .map(|(index, _)| cursor + index)
                .unwrap_or(browse.input.len());
            spans.push(Span::styled(
                browse.input[cursor..cursor_char_end].to_string(),
                Style::default().fg(bg).bg(theme::FG),
            ));
            if cursor_char_end < browse.input.len() {
                spans.push(Span::styled(
                    browse.input[cursor_char_end..].to_string(),
                    text_style,
                ));
            }
        } else {
            spans.push(Span::styled(" ", Style::default().fg(bg).bg(theme::FG)));
        }
    }
    let count = scored_matches(&browse.input, browse.category_filter).len();
    let right = if browse.input.trim().is_empty() {
        format!("{} commands", commands().len())
    } else if count == 1 {
        "1 match".to_owned()
    } else {
        format!("{count} matches")
    };
    let used = spans_width(&spans);
    let right_width = right.chars().count();
    if used + right_width + RIGHT_LABEL_PAD <= area.width as usize {
        let pad = area.width as usize - used - right_width - RIGHT_LABEL_PAD;
        spans.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
        spans.push(Span::styled(
            right,
            if count == 0 {
                theme::error().bg(bg)
            } else {
                theme::muted().bg(bg)
            },
        ));
        spans.push(Span::styled(
            " ".repeat(RIGHT_LABEL_PAD),
            Style::default().bg(bg),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

fn render_palette_list(
    frame: &mut Frame<'_>,
    area: Rect,
    browse: &BrowseState,
    matches: &[MatchHit],
    bg: Color,
) {
    let capacity = area.height as usize;
    if capacity == 0 {
        return;
    }
    if matches.is_empty() {
        let lines = vec![
            Line::from(vec![Span::raw("")]),
            Line::from(vec![
                Span::raw("   "),
                Span::styled("No commands match.", theme::dim().bg(bg)),
            ]),
            Line::from(vec![Span::raw("")]),
            Line::from(vec![
                Span::raw("   "),
                Span::styled(
                    "Try a shorter query, or press Esc to clear.",
                    theme::dim().bg(bg),
                ),
            ]),
        ];
        frame.render_widget(Paragraph::new(lines).style(Style::default().bg(bg)), area);
        return;
    }

    let rows = build_list_rows(browse, matches);
    // Find the slice that keeps the selected command on screen.
    let selected_row = rows
        .iter()
        .position(|row| matches!(row, BrowseRow::Command { match_index, .. } if *match_index == browse.selected))
        .unwrap_or(0);
    let start = compute_visible_start(selected_row, rows.len(), capacity);
    let end = (start + capacity).min(rows.len());

    let mut lines = Vec::with_capacity(capacity);
    let width = area.width as usize;
    for row in &rows[start..end] {
        lines.push(render_browse_row(row, browse.selected, width, bg));
    }
    // Scroll affordance when there are rows above/below the visible slice.
    if !lines.is_empty() {
        if start > 0 {
            lines[0] = overlay_scroll_indicator(lines.remove(0), '\u{25B4}', width, bg);
        }
        if end < rows.len() {
            let last = lines.len() - 1;
            let line = lines.remove(last);
            lines.insert(last, overlay_scroll_indicator(line, '\u{25BE}', width, bg));
        }
    }

    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(bg))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_palette_footer(frame: &mut Frame<'_>, area: Rect, browse: &BrowseState, bg: Color) {
    let separator = Span::styled("  ·  ", theme::dim().bg(bg));
    let mut spans = vec![
        Span::styled(" ", theme::dim().bg(bg)),
        Span::styled("↑↓", theme::accent().bg(bg)),
        Span::styled(" navigate", theme::dim().bg(bg)),
        separator.clone(),
        Span::styled("⏎", theme::accent().bg(bg)),
        Span::styled(" run", theme::dim().bg(bg)),
        separator.clone(),
        Span::styled("Tab", theme::accent().bg(bg)),
        Span::styled(" category", theme::dim().bg(bg)),
        separator.clone(),
        Span::styled("Esc", theme::accent().bg(bg)),
        Span::styled(
            if browse.category_filter.is_some() {
                " clear filter"
            } else {
                " close"
            },
            theme::dim().bg(bg),
        ),
    ];
    let used = spans_width(&spans);
    if (used + 2) <= area.width as usize {
        spans.push(Span::styled(
            " ".repeat(area.width as usize - used),
            Style::default().bg(bg),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        area,
    );
}

#[derive(Clone)]
enum BrowseRow {
    /// A blank visual break above a section header.
    Spacer,
    /// A category section header. Not selectable.
    Header { label: String },
    /// A real command row. `match_index` is the index into the matches
    /// slice (used to figure out the selected command).
    Command { match_index: usize, hit: MatchHit },
}

fn build_list_rows(browse: &BrowseState, matches: &[MatchHit]) -> Vec<BrowseRow> {
    let mut rows: Vec<BrowseRow> = Vec::new();
    let scoring = !browse.input.trim().is_empty();

    let push_header = |rows: &mut Vec<BrowseRow>, label: String| {
        if !rows.is_empty() {
            rows.push(BrowseRow::Spacer);
        }
        rows.push(BrowseRow::Header { label });
    };

    if scoring {
        // Scored search: pure flat list, no headers. The bottom context
        // strip carries the category for the selected hit.
        for (idx, hit) in matches.iter().enumerate() {
            rows.push(BrowseRow::Command {
                match_index: idx,
                hit: *hit,
            });
        }
        return rows;
    }

    if browse.category_filter.is_none() {
        // Empty input: curated quick row first, then every category.
        let essential_indices: Vec<usize> = ESSENTIALS
            .iter()
            .filter_map(|id| matches.iter().position(|hit| hit.command.id == *id))
            .collect();
        if !essential_indices.is_empty() {
            push_header(&mut rows, "Quick".to_owned());
            for idx in &essential_indices {
                rows.push(BrowseRow::Command {
                    match_index: *idx,
                    hit: matches[*idx],
                });
            }
        }
        let mut last_category: Option<Category> = None;
        for (idx, hit) in matches.iter().enumerate() {
            if essential_indices.contains(&idx) {
                continue;
            }
            let category = category_for(hit.command.id);
            if Some(category) != last_category {
                push_header(&mut rows, category.label().to_owned());
                last_category = Some(category);
            }
            rows.push(BrowseRow::Command {
                match_index: idx,
                hit: *hit,
            });
        }
    } else {
        // Category filter active, no input.
        if let Some(category) = browse.category_filter {
            push_header(&mut rows, category.label().to_owned());
        }
        for (idx, hit) in matches.iter().enumerate() {
            rows.push(BrowseRow::Command {
                match_index: idx,
                hit: *hit,
            });
        }
    }
    rows
}

fn render_browse_row(
    row: &BrowseRow,
    selected_index: usize,
    width: usize,
    bg: Color,
) -> Line<'static> {
    match row {
        BrowseRow::Spacer => Line::from(Span::styled(String::new(), Style::default().bg(bg))),
        BrowseRow::Header { label } => {
            // Subtle uppercase section label, no horizontal rule — the
            // single blank row before it (added by build_list_rows) gives
            // the visual break.
            let text = format!("   {}", label.to_uppercase());
            Line::from(Span::styled(
                text,
                theme::dim().bg(bg).add_modifier(Modifier::BOLD),
            ))
        }
        BrowseRow::Command { match_index, hit } => {
            let selected = *match_index == selected_index;
            let command = hit.command;
            let prefix_style = if selected {
                theme::accent().bg(bg)
            } else {
                theme::dim().bg(bg)
            };
            let title_style = if selected {
                theme::accent().bg(bg).add_modifier(Modifier::BOLD)
            } else {
                theme::fg().bg(bg)
            };
            let hint_style = if selected {
                theme::fg().bg(bg)
            } else {
                theme::muted().bg(bg)
            };

            let title_column = 26;
            let title_display = pad_to(command.title, title_column);
            let mut spans = vec![Span::styled(
                if selected { " ▎ " } else { "   " },
                prefix_style,
            )];
            push_highlighted_title(&mut spans, &title_display, hit.highlight, title_style, bg);
            let hint_room = width.saturating_sub(3 + title_column + 2);
            let hint = truncate(command.hint, hint_room);
            spans.push(Span::styled(format!("  {hint}"), hint_style));
            // Pad the row to the full width so the selected-row background
            // tint extends all the way to the right edge.
            let used = spans_width(&spans);
            if used + 1 < width {
                spans.push(Span::styled(
                    " ".repeat(width - used),
                    Style::default().bg(bg),
                ));
            }
            Line::from(spans)
        }
    }
}

fn push_highlighted_title(
    spans: &mut Vec<Span<'static>>,
    title: &str,
    highlight: Option<(usize, usize)>,
    base: Style,
    bg: Color,
) {
    let Some((start, len)) = highlight else {
        spans.push(Span::styled(title.to_owned(), base));
        return;
    };
    let end = (start + len).min(title.len());
    if start >= title.len() {
        spans.push(Span::styled(title.to_owned(), base));
        return;
    }
    if start > 0 {
        spans.push(Span::styled(title[..start].to_owned(), base));
    }
    spans.push(Span::styled(
        title[start..end].to_owned(),
        theme::accent().bg(bg).add_modifier(Modifier::BOLD),
    ));
    if end < title.len() {
        spans.push(Span::styled(title[end..].to_owned(), base));
    }
}

fn overlay_scroll_indicator(
    line: Line<'static>,
    glyph: char,
    width: usize,
    bg: Color,
) -> Line<'static> {
    let mut spans = line.spans;
    let used = spans
        .iter()
        .map(|s| s.content.chars().count())
        .sum::<usize>();
    if used + 2 < width {
        let pad = width - used - 2;
        spans.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
        spans.push(Span::styled(format!(" {glyph} "), theme::dim().bg(bg)));
    }
    Line::from(spans)
}

fn compute_visible_start(selected: usize, total: usize, capacity: usize) -> usize {
    if total <= capacity {
        return 0;
    }
    let half = capacity / 2;
    if selected < half {
        0
    } else if selected + half >= total {
        total.saturating_sub(capacity)
    } else {
        selected.saturating_sub(half)
    }
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.content.chars().count()).sum()
}

fn blank_line(bg: Color) -> Line<'static> {
    Line::from(Span::styled(String::new(), Style::default().bg(bg)))
}

fn blank_paragraph(bg: Color) -> Paragraph<'static> {
    Paragraph::new("").style(Style::default().bg(bg))
}

fn pad_to(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        s.chars().take(width).collect()
    } else {
        let mut out = String::with_capacity(width);
        out.push_str(s);
        for _ in 0..(width - len) {
            out.push(' ');
        }
        out
    }
}

fn truncate(s: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= width {
        return s.to_owned();
    }
    if width <= 1 {
        return "…".to_owned();
    }
    let mut out: String = chars[..width - 1].iter().collect();
    out.push('…');
    out
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, detail: &DetailState) {
    let bg = theme::composer_bg_color();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1), // title bar
            Constraint::Length(1), // hint subtitle (or blank)
            Constraint::Length(1), // spacer
            Constraint::Min(1),    // body
            Constraint::Length(1), // footer
        ])
        .split(area);

    let mut header_spans = vec![
        Span::styled(format!("{} ", theme::STRIPE_GLYPH), theme::accent().bg(bg)),
        Span::styled(
            detail.title.clone(),
            theme::accent().bg(bg).add_modifier(Modifier::BOLD),
        ),
    ];
    let kind_label = format!(" {} ", category_for(detail.command_id).label());
    let used = spans_width(&header_spans) + kind_label.chars().count();
    if used + 2 <= chunks[0].width as usize {
        let pad = chunks[0].width as usize - used;
        header_spans.push(Span::styled(" ".repeat(pad), Style::default().bg(bg)));
        header_spans.push(Span::styled(kind_label, theme::muted().bg(bg)));
    }
    frame.render_widget(
        Paragraph::new(Line::from(header_spans)).style(Style::default().bg(bg)),
        chunks[0],
    );

    let hint_text = detail.hint.clone().unwrap_or_else(|| "—".to_owned());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("  ", Style::default().bg(bg)),
            Span::styled(hint_text, theme::dim().bg(bg)),
        ]))
        .style(Style::default().bg(bg)),
        chunks[1],
    );

    frame.render_widget(blank_paragraph(bg), chunks[2]);

    let body_lines: Vec<Line<'static>> = match &detail.data {
        DetailData::Rows(rows) => rows
            .iter()
            .enumerate()
            .flat_map(|(idx, row)| {
                let selected = idx == detail.selected;
                let prefix_style = if selected {
                    theme::accent().bg(bg)
                } else {
                    theme::dim().bg(bg)
                };
                let title_style = if selected {
                    theme::accent().bg(bg).add_modifier(Modifier::BOLD)
                } else {
                    theme::fg().bg(bg)
                };
                let mut out = vec![Line::from(vec![
                    Span::styled(if selected { " ▎ " } else { "   " }, prefix_style),
                    Span::styled(row.title.clone(), title_style),
                ])];
                if let Some(subtitle) = row.subtitle.clone() {
                    out.push(Line::from(vec![
                        Span::raw("     "),
                        Span::styled(subtitle, theme::muted().bg(bg)),
                    ]));
                }
                out
            })
            .collect(),
        DetailData::Body(lines) => lines
            .iter()
            .map(|text| {
                Line::from(vec![
                    Span::raw("  "),
                    Span::styled(text.clone(), theme::fg().bg(bg)),
                ])
            })
            .collect(),
        DetailData::Empty(text) => vec![
            blank_line(bg),
            Line::from(vec![
                Span::raw("   "),
                Span::styled(text.clone(), theme::dim().bg(bg)),
            ]),
        ],
    };
    frame.render_widget(
        Paragraph::new(body_lines)
            .style(Style::default().bg(bg))
            .wrap(Wrap { trim: false }),
        chunks[3],
    );

    let separator = Span::styled("  ·  ", theme::dim().bg(bg));
    let mut spans = vec![
        Span::styled(" ", Style::default().bg(bg)),
        Span::styled("↑↓", theme::accent().bg(bg)),
        Span::styled(" navigate", theme::dim().bg(bg)),
        separator.clone(),
        Span::styled("⏎", theme::accent().bg(bg)),
        Span::styled(" select", theme::dim().bg(bg)),
        separator.clone(),
        Span::styled("Esc", theme::accent().bg(bg)),
        Span::styled(" back to palette", theme::dim().bg(bg)),
    ];
    let used = spans_width(&spans);
    if used + 2 <= chunks[4].width as usize {
        spans.push(Span::styled(
            " ".repeat(chunks[4].width as usize - used),
            Style::default().bg(bg),
        ));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).style(Style::default().bg(bg)),
        chunks[4],
    );
}

// ---------------------------------------------------------------------------
// Helpers shared by entries
// ---------------------------------------------------------------------------

/// Build an [`OpenOutcome::Detail`] with [`DetailData::Rows`].
pub(crate) fn rows_detail(
    command_id: &'static str,
    title: impl Into<String>,
    hint: Option<&str>,
    rows: Vec<DetailRow>,
) -> OpenOutcome {
    OpenOutcome::Detail(DetailState {
        command_id,
        title: title.into(),
        hint: hint.map(str::to_owned),
        data: DetailData::Rows(rows),
        selected: 0,
    })
}

/// Build an [`OpenOutcome::Detail`] with [`DetailData::Body`].
pub(crate) fn body_detail(
    command_id: &'static str,
    title: impl Into<String>,
    hint: Option<&str>,
    body: Vec<String>,
) -> OpenOutcome {
    OpenOutcome::Detail(DetailState {
        command_id,
        title: title.into(),
        hint: hint.map(str::to_owned),
        data: DetailData::Body(body),
        selected: 0,
    })
}

/// Build a "no data" detail view.
pub(crate) fn empty_detail(
    command_id: &'static str,
    title: impl Into<String>,
    text: impl Into<String>,
) -> OpenOutcome {
    OpenOutcome::Detail(DetailState {
        command_id,
        title: title.into(),
        hint: None,
        data: DetailData::Empty(text.into()),
        selected: 0,
    })
}

/// Build an error detail view from a [`CliError`].
pub(crate) fn error_detail(
    command_id: &'static str,
    title: impl Into<String>,
    error: crate::CliError,
) -> OpenOutcome {
    OpenOutcome::Detail(DetailState {
        command_id,
        title: title.into(),
        hint: None,
        data: DetailData::Empty(format!("{} ({})", error.message, error.code)),
        selected: 0,
    })
}

pub(crate) fn string_field(value: &JsonValue, key: &str) -> String {
    value
        .get(key)
        .and_then(JsonValue::as_str)
        .unwrap_or_default()
        .to_owned()
}

pub(crate) fn array_field<'a>(value: &'a JsonValue, key: &str) -> &'a [JsonValue] {
    value
        .get(key)
        .and_then(JsonValue::as_array)
        .map(|v| v.as_slice())
        .unwrap_or(&[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};
    use ratatui::{backend::TestBackend, buffer::Buffer, Terminal};

    #[test]
    fn registry_lists_every_expected_command() {
        let ids: Vec<&str> = commands().iter().map(|cmd| cmd.id).collect();
        for expected in [
            "sessions",
            "new",
            "files",
            "git",
            "processes",
            "providers",
            "auth",
            "agents",
            "skills",
            "context",
            "recovery",
            "usage",
            "events",
            "settings",
            "settings agent-tooling",
            "provider login",
            "session list",
            "remote login",
            "register",
            "quit",
        ] {
            assert!(
                ids.contains(&expected),
                "registry missing command `{expected}` (have: {ids:?})",
            );
        }
    }

    #[test]
    fn filter_matches_substring_case_insensitive() {
        let matches = filtered("PROV");
        assert!(
            matches.iter().any(|cmd| cmd.id == "providers"),
            "expected `providers` match",
        );
        assert!(
            matches.iter().any(|cmd| cmd.id == "provider login"),
            "expected nested `provider login` match",
        );
        let empty = filtered("");
        assert_eq!(empty.len(), commands().len());
        let none = filtered("zzzzzz-no-such-command");
        assert!(none.is_empty());
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn key_char(ch: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(ch), KeyModifiers::NONE)
    }

    fn selected_id(browse: &BrowseState) -> &'static str {
        scored_matches(&browse.input, browse.category_filter)
            .get(browse.selected)
            .map(|hit| hit.command.id)
            .expect("selected command")
    }

    fn buffer_rows(buffer: &Buffer) -> Vec<String> {
        buffer
            .content
            .chunks(buffer.area.width as usize)
            .map(|row| row.iter().map(|cell| cell.symbol()).collect::<String>())
            .collect()
    }

    fn rendered_browse_rows(width: u16, height: u16) -> Vec<String> {
        let mut app = super::super::app::test_only_empty_app();
        app.palette = Some(open());
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal
            .draw(|frame| render(frame, Rect::new(0, 0, width, height), &app))
            .expect("draw");
        buffer_rows(terminal.backend().buffer())
    }

    fn modified_key(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn esc_in_browse_closes_palette() {
        let mut app = super::super::app::test_only_empty_app();
        let globals = super::super::app::test_only_globals();
        app.palette = Some(open());
        let outcome = handle_key(&mut app, key(KeyCode::Esc), &globals);
        assert!(matches!(outcome, KeyResult::Close { status: None }));
    }

    #[test]
    fn typing_filters_browse_list_and_selects_top() {
        let mut app = super::super::app::test_only_empty_app();
        let globals = super::super::app::test_only_globals();
        app.palette = Some(open());

        // Type "prov" and confirm browse state reflects it.
        for ch in "prov".chars() {
            let _ = handle_key(&mut app, key_char(ch), &globals);
        }
        match app.palette.as_ref().expect("palette open") {
            PaletteState::Browse(browse) => {
                assert_eq!(browse.input, "prov");
                let matches = filtered(&browse.input);
                assert!(matches.iter().any(|cmd| cmd.id == "providers"));
            }
            PaletteState::Detail(_) => panic!("expected browse state"),
        }
    }

    #[test]
    fn browse_input_supports_word_and_line_delete_shortcuts() {
        let mut app = super::super::app::test_only_empty_app();
        let globals = super::super::app::test_only_globals();
        app.palette = Some(PaletteState::Browse(BrowseState {
            input: "provider login".into(),
            input_cursor: "provider login".len(),
            input_desired_column: None,
            selected: 3,
            category_filter: None,
        }));

        let outcome = handle_key(
            &mut app,
            modified_key(KeyCode::Backspace, KeyModifiers::ALT),
            &globals,
        );

        assert!(matches!(outcome, KeyResult::Continue));
        match app.palette.as_ref().expect("palette open") {
            PaletteState::Browse(browse) => {
                assert_eq!(browse.input, "provider ");
                assert_eq!(browse.selected, 0);
            }
            PaletteState::Detail(_) => panic!("expected browse state"),
        }

        let outcome = handle_key(
            &mut app,
            modified_key(KeyCode::Backspace, KeyModifiers::META),
            &globals,
        );

        assert!(matches!(outcome, KeyResult::Continue));
        match app.palette.as_ref().expect("palette open") {
            PaletteState::Browse(browse) => {
                assert!(browse.input.is_empty());
                assert_eq!(browse.selected, 0);
            }
            PaletteState::Detail(_) => panic!("expected browse state"),
        }
    }

    #[test]
    fn scored_matches_rank_prefix_above_substring() {
        let hits = scored_matches("git", None);
        let first = hits.first().expect("matches");
        assert_eq!(first.command.id, "git", "exact id match should win");
        let next: Vec<&str> = hits.iter().take(4).map(|h| h.command.id).collect();
        assert!(
            next.iter().all(|id| id.starts_with("git") || *id == "git"),
            "top hits should be git*-prefixed, got {next:?}",
        );
    }

    #[test]
    fn acronym_matches_multiword_command() {
        let hits = scored_matches("pl", None);
        assert!(
            hits.iter().any(|hit| hit.command.id == "project list"),
            "acronym `pl` should surface `project list`",
        );
    }

    #[test]
    fn category_filter_restricts_results() {
        let hits = scored_matches("", Some(Category::Git));
        assert!(!hits.is_empty(), "git category should have entries");
        assert!(
            hits.iter()
                .all(|hit| category_for(hit.command.id) == Category::Git),
            "category filter should restrict by category",
        );
    }

    #[test]
    fn tab_cycles_into_first_category() {
        let mut app = super::super::app::test_only_empty_app();
        let globals = super::super::app::test_only_globals();
        app.palette = Some(open());
        let _ = handle_key(&mut app, key(KeyCode::Tab), &globals);
        match app.palette.as_ref().expect("palette open") {
            PaletteState::Browse(browse) => {
                assert_eq!(browse.category_filter, Some(Category::Session));
            }
            PaletteState::Detail(_) => panic!("expected browse"),
        }
        // Esc clears the category filter rather than closing the palette.
        let outcome = handle_key(&mut app, key(KeyCode::Esc), &globals);
        assert!(matches!(outcome, KeyResult::Continue));
        match app.palette.as_ref().expect("palette open") {
            PaletteState::Browse(browse) => assert!(browse.category_filter.is_none()),
            PaletteState::Detail(_) => panic!("expected browse"),
        }
    }

    #[test]
    fn up_from_first_session_command_moves_into_quick_section() {
        let mut app = super::super::app::test_only_empty_app();
        let globals = super::super::app::test_only_globals();
        let selected = scored_matches("", None)
            .iter()
            .position(|hit| hit.command.id == "conversation answer")
            .expect("conversation answer command");
        app.palette = Some(PaletteState::Browse(BrowseState {
            input: String::new(),
            input_cursor: 0,
            input_desired_column: None,
            selected,
            category_filter: None,
        }));

        let outcome = handle_key(&mut app, key(KeyCode::Up), &globals);

        assert!(matches!(outcome, KeyResult::Continue));
        match app.palette.as_ref().expect("palette open") {
            PaletteState::Browse(browse) => assert_eq!(selected_id(browse), "settings"),
            PaletteState::Detail(_) => panic!("expected browse"),
        }
    }

    #[test]
    fn down_from_quick_section_moves_into_first_session_command() {
        let mut app = super::super::app::test_only_empty_app();
        let globals = super::super::app::test_only_globals();
        let selected = scored_matches("", None)
            .iter()
            .position(|hit| hit.command.id == "settings")
            .expect("settings command");
        app.palette = Some(PaletteState::Browse(BrowseState {
            input: String::new(),
            input_cursor: 0,
            input_desired_column: None,
            selected,
            category_filter: None,
        }));

        let outcome = handle_key(&mut app, key(KeyCode::Down), &globals);

        assert!(matches!(outcome, KeyResult::Continue));
        match app.palette.as_ref().expect("palette open") {
            PaletteState::Browse(browse) => {
                assert_eq!(selected_id(browse), "conversation answer");
            }
            PaletteState::Detail(_) => panic!("expected browse"),
        }
    }

    #[test]
    fn browse_header_renders_without_decorative_stripe() {
        let rows = rendered_browse_rows(100, 30);
        let title_row = rows
            .iter()
            .find(|row| row.contains("Command Palette"))
            .expect("title row");
        let input_row = rows
            .iter()
            .find(|row| row.contains("Search commands"))
            .expect("input row");

        assert!(!title_row.contains(theme::STRIPE_GLYPH));
        assert!(!input_row.contains(theme::STRIPE_GLYPH));
    }

    #[test]
    fn command_count_label_keeps_right_padding() {
        let rows = rendered_browse_rows(100, 30);
        let count_row = rows
            .iter()
            .find(|row| row.contains("commands"))
            .expect("count row");

        assert!(
            count_row.contains("commands  "),
            "expected two cells after count label: `{count_row}`",
        );
    }

    #[test]
    fn quit_command_returns_quit_outcome() {
        let mut app = super::super::app::test_only_empty_app();
        let globals = super::super::app::test_only_globals();
        // Open palette, filter to `quit`, press enter.
        app.palette = Some(open());
        for ch in "quit".chars() {
            let _ = handle_key(&mut app, key_char(ch), &globals);
        }
        let outcome = handle_key(&mut app, key(KeyCode::Enter), &globals);
        assert!(matches!(outcome, KeyResult::Quit));
    }
}
