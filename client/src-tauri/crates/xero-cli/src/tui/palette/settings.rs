//! `settings` — terminal-safe settings and diagnostics surfaces.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::{
    super::app::App, cli_command_outcome, rows_detail, CliCommandMode, CliCommandSpec,
    CommandScope, DetailOutcome, DetailRow, DetailState, OpenOutcome,
};

const ID: &str = "settings";

struct SettingsAction {
    title: &'static str,
    subtitle: &'static str,
    spec: CliCommandSpec,
}

static SETTINGS_ACTIONS: &[SettingsAction] = &[
    SettingsAction {
        title: "settings agent-tooling",
        subtitle: "Agent tool preference defaults and model overrides.",
        spec: run(&["settings", "agent-tooling"], CommandScope::None),
    },
    SettingsAction {
        title: "settings browser-control",
        subtitle: "Browser-control preference used by compatible runs.",
        spec: run(&["settings", "browser-control"], CommandScope::None),
    },
    SettingsAction {
        title: "settings soul",
        subtitle: "Behavior preset selection.",
        spec: run(&["settings", "soul"], CommandScope::None),
    },
    SettingsAction {
        title: "environment status",
        subtitle: "Local environment discovery status and diagnostic counts.",
        spec: run(&["environment", "status"], CommandScope::None),
    },
    SettingsAction {
        title: "environment profile",
        subtitle: "Discovered local tools and environment summary.",
        spec: run(&["environment", "profile"], CommandScope::None),
    },
    SettingsAction {
        title: "environment user-tools",
        subtitle: "User-saved terminal tools in global app data.",
        spec: run(&["environment", "user-tools"], CommandScope::None),
    },
    SettingsAction {
        title: "provider list",
        subtitle: "Configured provider profiles, auth kind, and model defaults.",
        spec: run(&["provider", "list"], CommandScope::None),
    },
    SettingsAction {
        title: "provider doctor",
        subtitle: "Provider auth and model configuration diagnostics.",
        spec: run(&["provider", "doctor"], CommandScope::None),
    },
    SettingsAction {
        title: "mcp list",
        subtitle: "Configured MCP servers.",
        spec: run(&["mcp", "list"], CommandScope::None),
    },
    SettingsAction {
        title: "mcp status",
        subtitle: "MCP readiness and auth state.",
        spec: run(&["mcp", "status"], CommandScope::None),
    },
    SettingsAction {
        title: "notification routes",
        subtitle: "Approval notification routes for this project.",
        spec: run(&["notification", "routes"], CommandScope::ProjectRequired),
    },
    SettingsAction {
        title: "usage summary",
        subtitle: "Token and cost summary for this project.",
        spec: run(&["usage", "summary"], CommandScope::ProjectRequired),
    },
    SettingsAction {
        title: "project-state list",
        subtitle: "Project state backups in OS app-data.",
        spec: run(&["project-state", "list"], CommandScope::ProjectRequired),
    },
    SettingsAction {
        title: "tool-pack doctor",
        subtitle: "Domain tool-pack prerequisite checks.",
        spec: run(&["tool-pack", "doctor"], CommandScope::None),
    },
];

const fn run(args: &'static [&'static str], scope: CommandScope) -> CliCommandSpec {
    CliCommandSpec {
        args,
        scope,
        mode: CliCommandMode::Run,
    }
}

pub fn open(_globals: &GlobalOptions, _app: &mut App) -> OpenOutcome {
    rows_detail(
        ID,
        "Settings",
        Some("enter open   esc back"),
        SETTINGS_ACTIONS
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
    let Some(action) = SETTINGS_ACTIONS.get(detail.selected) else {
        return DetailOutcome::Stay;
    };
    if let OpenOutcome::Detail(next) =
        cli_command_outcome(ID, action.title, action.spec, globals, app)
    {
        *detail = next;
    }
    DetailOutcome::Stay
}
