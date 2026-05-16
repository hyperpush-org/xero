//! `events` — raw event log for the active run.

use crossterm::event::KeyEvent;

use crate::GlobalOptions;

use super::{
    super::app::App, empty_detail, rows_detail, DetailOutcome, DetailRow, DetailState, OpenOutcome,
};

const ID: &str = "events";

pub fn open(_globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let Some(run) = app.run_detail.as_ref() else {
        return empty_detail(
            ID,
            "Events",
            "No active run. Start a prompt to populate event history.",
        );
    };
    if run.events.is_empty() {
        return empty_detail(ID, "Events", "Active run has no events yet.");
    }
    let rows = run
        .events
        .iter()
        .map(|event| DetailRow {
            title: if event.event_kind.is_empty() {
                "event".to_owned()
            } else {
                event.event_kind.clone()
            },
            subtitle: if event.summary.is_empty() {
                None
            } else {
                Some(event.summary.clone())
            },
            payload: serde_json::Value::Null,
        })
        .collect::<Vec<_>>();
    rows_detail(ID, "Events", Some("esc back"), rows)
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}
