//! `processes` — list managed PTY sessions for the active project.

use crossterm::event::KeyEvent;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "processes";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let Some(project_id) = app.project.project_id.as_deref() else {
        return empty_detail(
            ID,
            "Processes",
            "No project bound. Use `register` to add this directory.",
        );
    };
    let value = match invoke_json(globals, &["process", "list", "--project-id", project_id]) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Processes", error),
    };
    let rows = array_field(&value, "sessions")
        .iter()
        .map(|session| {
            let id = string_field(session, "sessionId");
            let label = string_field(session, "label");
            let status = string_field(session, "status");
            DetailRow {
                title: if label.is_empty() { id.clone() } else { label },
                subtitle: Some(format!(
                    "{}{}",
                    id,
                    if status.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", status)
                    }
                )),
                payload: session.clone(),
            }
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return empty_detail(ID, "Processes", "No process sessions on this project.");
    }
    rows_detail(ID, "Processes", Some("esc back"), rows)
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}
