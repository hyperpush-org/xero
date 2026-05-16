//! `context` — surface the live run's transcript/messages/events when
//! present, or a brief explainer otherwise. The full Context screen
//! (todos / plan / memory) is reached through additional palette
//! sub-entries planned for M3.

use crossterm::event::KeyEvent;

use crate::GlobalOptions;

use super::{super::app::App, body_detail, empty_detail, DetailOutcome, DetailState, OpenOutcome};

const ID: &str = "context";

pub fn open(_globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let Some(detail) = app.run_detail.as_ref() else {
        return empty_detail(
            ID,
            "Context",
            "No active run. Start a prompt to populate run context.",
        );
    };
    let mut body = Vec::new();
    body.push(format!("run: {}", detail.run_id));
    body.push(format!("status: {}", detail.status));
    body.push(format!("messages: {}", detail.messages.len()));
    body.push(format!("events: {}", detail.events.len()));
    if let Some(session_id) = app.active_session_id.as_deref() {
        body.push(format!("session: {}", session_id));
    }
    if let Some(project_id) = app.project.project_id.as_deref() {
        body.push(format!("project: {}", project_id));
    }
    body_detail(ID, "Context", Some("esc back"), body)
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}
