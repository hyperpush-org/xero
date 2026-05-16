//! `auth` — terminal-safe authentication and credential surfaces.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::{
    super::app::App, cli_command_outcome, rows_detail, CliCommandMode, CliCommandSpec,
    CommandScope, DetailOutcome, DetailRow, DetailState, OpenOutcome,
};

const ID: &str = "auth";

struct AuthAction {
    title: &'static str,
    subtitle: &'static str,
    spec: CliCommandSpec,
}

static AUTH_ACTIONS: &[AuthAction] = &[
    AuthAction {
        title: "provider list",
        subtitle: "Configured provider profiles and credential kinds.",
        spec: run(&["provider", "list"], CommandScope::None),
    },
    AuthAction {
        title: "provider login",
        subtitle: "Create a provider auth/API-key profile from the composer.",
        spec: preview(&["provider", "login"], CommandScope::None),
    },
    AuthAction {
        title: "provider remove",
        subtitle: "Remove a provider profile from the composer.",
        spec: preview(&["provider", "remove"], CommandScope::None),
    },
    AuthAction {
        title: "provider doctor",
        subtitle: "Check provider auth and model readiness.",
        spec: run(&["provider", "doctor"], CommandScope::None),
    },
    AuthAction {
        title: "remote login",
        subtitle: "Start or poll remote GitHub login from the composer.",
        spec: preview(&["remote", "login"], CommandScope::None),
    },
    AuthAction {
        title: "remote devices",
        subtitle: "List remote-authorized devices.",
        spec: run(&["remote", "devices"], CommandScope::None),
    },
    AuthAction {
        title: "mcp status",
        subtitle: "Show configured MCP auth/readiness status.",
        spec: run(&["mcp", "status"], CommandScope::None),
    },
    AuthAction {
        title: "mcp login",
        subtitle: "Run an MCP login flow from the composer.",
        spec: preview(&["mcp", "login"], CommandScope::None),
    },
];

const fn run(args: &'static [&'static str], scope: CommandScope) -> CliCommandSpec {
    CliCommandSpec {
        args,
        scope,
        mode: CliCommandMode::Run,
    }
}

const fn preview(args: &'static [&'static str], scope: CommandScope) -> CliCommandSpec {
    CliCommandSpec {
        args,
        scope,
        mode: CliCommandMode::Preview,
    }
}

pub fn open(_globals: &GlobalOptions, _app: &mut App) -> OpenOutcome {
    rows_detail(
        ID,
        "Auth",
        Some("enter open   esc back"),
        AUTH_ACTIONS
            .iter()
            .map(|action| DetailRow {
                title: action.title.to_owned(),
                subtitle: Some(action.subtitle.to_owned()),
                payload: JsonValue::Null,
            })
            .collect(),
    )
}

pub fn handle_key(
    app: &mut App,
    detail: &mut DetailState,
    key: KeyEvent,
    globals: &GlobalOptions,
) -> DetailOutcome {
    if !matches!(key.code, KeyCode::Enter) {
        return DetailOutcome::Stay;
    }
    let Some(action) = AUTH_ACTIONS.get(detail.selected) else {
        return DetailOutcome::Stay;
    };
    if let OpenOutcome::Detail(next) =
        cli_command_outcome(ID, action.title, action.spec, globals, app)
    {
        *detail = next;
    }
    DetailOutcome::Stay
}
