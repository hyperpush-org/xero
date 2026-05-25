//! `new` — start or focus a chat surface. One-shot: creates/focuses the
//! target through app-data and dismisses the palette. The next prompt the
//! user sends will use the selected session.

use crate::{
    project_cli::{
        ensure_global_computer_use_project, global_computer_use_root,
        GLOBAL_COMPUTER_USE_AGENT_SESSION_ID, GLOBAL_COMPUTER_USE_PROJECT_ID,
        GLOBAL_COMPUTER_USE_PROJECT_NAME,
    },
    GlobalOptions,
};

use super::{
    super::{
        app::{invoke_json, AgentEntry, App},
        project::ResolvedProject,
    },
    string_field, OpenOutcome,
};

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    open_with_kind(globals, app, "standard", "New Chat", "New session")
}

pub fn open_computer_use(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    if let Err(error) = app.discard_pending_attachments(globals) {
        return OpenOutcome::Closed {
            status: Some(format!(
                "Could not clear pending attachments: {} ({})",
                error.message, error.code
            )),
        };
    }
    if let Err(error) = ensure_global_computer_use_project(globals) {
        return OpenOutcome::Closed {
            status: Some(format!(
                "Could not open Computer Use: {} ({})",
                error.message, error.code
            )),
        };
    }

    let root = global_computer_use_root(globals);
    app.project = ResolvedProject {
        project_id: Some(GLOBAL_COMPUTER_USE_PROJECT_ID.to_owned()),
        root,
        branch: None,
        display_path: GLOBAL_COMPUTER_USE_PROJECT_NAME.to_owned(),
        registered: true,
    };
    let selected = app
        .agents
        .iter()
        .position(|agent| agent.definition_id == "computer_use")
        .unwrap_or_else(|| {
            app.agents.push(AgentEntry {
                definition_id: "computer_use".to_owned(),
                display_name: GLOBAL_COMPUTER_USE_PROJECT_NAME.to_owned(),
            });
            app.agents.len().saturating_sub(1)
        });
    app.selected_agent = selected;
    app.reset_for_new_session(Some(GLOBAL_COMPUTER_USE_AGENT_SESSION_ID.to_owned()));
    super::super::app::sync_active_session_to_cloud_best_effort(globals, app);
    OpenOutcome::Closed {
        status: Some("Computer Use ready.".to_owned()),
    }
}

fn open_with_kind(
    globals: &GlobalOptions,
    app: &mut App,
    session_kind: &str,
    title: &str,
    status_label: &str,
) -> OpenOutcome {
    let Some(project_id) = app.project.project_id.clone() else {
        return OpenOutcome::Closed {
            status: Some("No project bound — `register` this directory first.".to_owned()),
        };
    };
    match invoke_json(
        globals,
        &[
            "session",
            "create",
            "--project-id",
            &project_id,
            "--title",
            title,
            "--session-kind",
            session_kind,
        ],
    ) {
        Ok(value) => {
            let session = value.get("session").cloned().unwrap_or(value);
            let session_id = string_field(&session, "agentSessionId");
            if let Err(error) = app.discard_pending_attachments(globals) {
                return OpenOutcome::Closed {
                    status: Some(format!(
                        "Could not clear pending attachments: {} ({})",
                        error.message, error.code
                    )),
                };
            }
            app.reset_for_new_session((!session_id.is_empty()).then_some(session_id.clone()));
            if !session_id.is_empty() {
                super::super::app::sync_active_session_to_cloud_best_effort(globals, app);
            }
            OpenOutcome::Closed {
                status: Some(if session_id.is_empty() {
                    format!("{status_label}.")
                } else {
                    format!("{status_label}: {}", session_id)
                }),
            }
        }
        Err(error) => OpenOutcome::Closed {
            status: Some(format!(
                "Could not start a new session: {} ({})",
                error.message, error.code
            )),
        },
    }
}
