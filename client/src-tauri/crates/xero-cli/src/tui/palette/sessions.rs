//! `sessions` — list project sessions; Enter sets the active session.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "sessions";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let Some(project_id) = app.project.project_id.as_deref() else {
        return empty_detail(
            ID,
            "Sessions",
            "No project bound. Use the `register` command to add this directory.",
        );
    };
    let value = match invoke_json(globals, &["session", "list", "--project-id", project_id]) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Sessions", error),
    };
    let rows = array_field(&value, "sessions")
        .iter()
        .map(session_to_row)
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return empty_detail(ID, "Sessions", "No sessions yet for this project.");
    }
    rows_detail(ID, "Sessions", Some("enter select   esc back"), rows)
}

pub fn handle_key(
    app: &mut App,
    detail: &mut DetailState,
    key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    if !matches!(key.code, KeyCode::Enter) {
        return DetailOutcome::Stay;
    }
    let super::DetailData::Rows(rows) = &detail.data else {
        return DetailOutcome::Stay;
    };
    let Some(row) = rows.get(detail.selected) else {
        return DetailOutcome::Stay;
    };
    let session_id = string_field(&row.payload, "agentSessionId");
    let title = string_field(&row.payload, "title");
    app.active_session_id = Some(session_id.clone());
    DetailOutcome::Close {
        status: Some(format!(
            "Active session: {} ({})",
            if title.is_empty() {
                "untitled"
            } else {
                title.as_str()
            },
            session_id,
        )),
    }
}

fn session_to_row(session: &JsonValue) -> DetailRow {
    let title = string_field(session, "title");
    let session_id = string_field(session, "agentSessionId");
    let status = string_field(session, "status");
    let updated_at = string_field(session, "updatedAt");
    DetailRow {
        title: if title.is_empty() {
            session_id.clone()
        } else {
            title
        },
        subtitle: Some(format!(
            "{}{}{}",
            session_id,
            if status.is_empty() {
                String::new()
            } else {
                format!(" · {}", status)
            },
            if updated_at.is_empty() {
                String::new()
            } else {
                format!(" · {}", updated_at)
            },
        )),
        payload: session.clone(),
    }
}
