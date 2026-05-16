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
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
    Frame,
};
use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::{app::App, theme};

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
    pub selected: usize,
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

/// Filter the registry by the current input — case-insensitive substring
/// match against id, title, or hint.
pub(crate) fn filtered(input: &str) -> Vec<&'static Command> {
    if input.trim().is_empty() {
        return COMMANDS.iter().collect();
    }
    let needle = input.to_lowercase();
    COMMANDS
        .iter()
        .filter(|cmd| {
            cmd.id.to_lowercase().contains(&needle)
                || cmd.title.to_lowercase().contains(&needle)
                || cmd.hint.to_lowercase().contains(&needle)
        })
        .collect()
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
    let matches = filtered(&browse.input);
    let count = matches.len();
    match key.code {
        KeyCode::Esc => KeyResult::Close { status: None },
        KeyCode::Up => {
            if count > 0 {
                browse.selected = browse.selected.saturating_sub(1);
            }
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::Down => {
            if count > 0 {
                browse.selected = (browse.selected + 1).min(count - 1);
            }
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::Backspace => {
            browse.input.pop();
            browse.selected = 0;
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::Char(ch) => {
            browse.input.push(ch);
            browse.selected = 0;
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
        }
        KeyCode::Enter => {
            let Some(command) = matches.get(browse.selected).copied() else {
                app.palette = Some(PaletteState::Browse(browse));
                return KeyResult::Continue;
            };
            activate_command(command, app, globals)
        }
        _ => {
            app.palette = Some(PaletteState::Browse(browse));
            KeyResult::Continue
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

/// Preferred overlay size for the current palette state. The caller centers
/// a rect of this size inside the viewport. The returned dimensions include
/// the 1-cell decorative halo on every side.
pub fn desired_size(app: &App, viewport: Rect) -> (u16, u16) {
    const MIN_WIDTH: u16 = 36;
    const MAX_WIDTH: u16 = 72;
    let width = viewport.width.saturating_mul(50) / 100;
    let width = width.clamp(MIN_WIDTH.min(viewport.width), MAX_WIDTH);
    let content_height = match app.palette.as_ref() {
        Some(PaletteState::Browse(browse)) => {
            let rows = filtered(&browse.input).len().max(1) as u16;
            // 2 rows for input + spacer + rows
            2 + rows
        }
        Some(PaletteState::Detail(detail)) => {
            // 1 header + 1 spacer + body lines + 1 hint
            let body = match &detail.data {
                DetailData::Rows(rows) => rows
                    .iter()
                    .map(|row| if row.subtitle.is_some() { 2 } else { 1 })
                    .sum::<usize>(),
                DetailData::Body(lines) => lines.len(),
                DetailData::Empty(_) => 1,
            }
            .max(1) as u16;
            3 + body
        }
        None => 0,
    };
    // +2 rows / +2 cols for the halo (1 on each side) plus the inner
    // vertical padding above/below the content.
    let max_height = viewport.height.saturating_mul(90) / 100;
    let height = (content_height + 2 * VERTICAL_PAD_ROWS + 2)
        .min(max_height)
        .max(7);
    (width, height)
}

pub fn render(frame: &mut Frame<'_>, area: Rect, app: &App) {
    let Some(state) = app.palette.as_ref() else {
        return;
    };
    if area.width <= 2 || area.height <= 2 {
        return;
    }
    // Paint a 1-cell decorative halo (lighter than composer bg) around
    // the surface so the overlay reads as a raised card.
    let border = Paragraph::new("").style(theme::overlay_border());
    frame.render_widget(border, area);

    // The surface sits inset by 1 cell on every side; the halo color
    // shows through on the resulting ring.
    let surface_area = Rect {
        x: area.x + 1,
        y: area.y + 1,
        width: area.width - 2,
        height: area.height - 2,
    };
    let surface = Paragraph::new("").style(theme::composer_bg());
    frame.render_widget(surface, surface_area);

    // Carve out a top-padded inner rect for the actual content so text
    // doesn't crowd the surface's top/bottom edges.
    let inner = inner_rect(surface_area);
    match state {
        PaletteState::Browse(browse) => render_browse(frame, inner, browse),
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

fn render_browse(frame: &mut Frame<'_>, area: Rect, browse: &BrowseState) {
    let bg = theme::composer_bg_color();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(2), Constraint::Min(0)])
        .split(area);

    let input_text = if browse.input.is_empty() {
        Span::styled("Type a command...", theme::dim().bg(bg))
    } else {
        Span::styled(browse.input.clone(), theme::fg().bg(bg))
    };
    let input_line = Line::from(vec![
        Span::styled(format!("{} ", theme::STRIPE_GLYPH), theme::accent().bg(bg)),
        input_text,
    ]);
    frame.render_widget(
        Paragraph::new(input_line).style(theme::composer_bg()),
        chunks[0],
    );

    let matches = filtered(&browse.input);
    let visible_capacity = chunks[1].height as usize;
    let mut lines = Vec::with_capacity(visible_capacity.max(1));
    if matches.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No commands match.",
            theme::dim().bg(bg),
        )));
    } else {
        let start = browse
            .selected
            .saturating_sub(visible_capacity.saturating_sub(1));
        let end = (start + visible_capacity).min(matches.len());
        for (idx, command) in matches.iter().enumerate().skip(start).take(end - start) {
            let row_style = if idx == browse.selected {
                theme::accent().bg(bg)
            } else {
                theme::fg().bg(bg)
            };
            lines.push(Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<14}", command.title), row_style),
                Span::raw("  "),
                Span::styled(command.hint, theme::muted().bg(bg)),
            ]));
        }
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(theme::composer_bg())
            .wrap(Wrap { trim: false }),
        chunks[1],
    );
}

fn render_detail(frame: &mut Frame<'_>, area: Rect, detail: &DetailState) {
    let bg = theme::composer_bg_color();
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area);

    let header = Line::from(vec![
        Span::styled(format!("{} ", theme::STRIPE_GLYPH), theme::accent().bg(bg)),
        Span::styled(detail.title.clone(), theme::accent().bg(bg)),
    ]);
    frame.render_widget(
        Paragraph::new(header).style(theme::composer_bg()),
        chunks[0],
    );
    frame.render_widget(Paragraph::new("").style(theme::composer_bg()), chunks[1]);

    let body_lines: Vec<Line<'static>> = match &detail.data {
        DetailData::Rows(rows) => rows
            .iter()
            .enumerate()
            .flat_map(|(idx, row)| {
                let title_style = if idx == detail.selected {
                    theme::accent().bg(bg)
                } else {
                    theme::fg().bg(bg)
                };
                let mut out = vec![Line::from(vec![
                    Span::raw("  "),
                    Span::styled(row.title.clone(), title_style),
                ])];
                if let Some(subtitle) = row.subtitle.clone() {
                    out.push(Line::from(vec![
                        Span::raw("    "),
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
        DetailData::Empty(text) => vec![Line::from(vec![
            Span::raw("  "),
            Span::styled(text.clone(), theme::dim().bg(bg)),
        ])],
    };
    frame.render_widget(
        Paragraph::new(body_lines)
            .style(theme::composer_bg())
            .wrap(Wrap { trim: false }),
        chunks[2],
    );

    let hint = detail
        .hint
        .clone()
        .unwrap_or_else(|| "esc back   ctrl+p /commands".to_owned());
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, theme::dim().bg(bg))))
            .style(theme::composer_bg()),
        chunks[3],
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
