//! `recovery` — list recent runs available for branching, rewind, export.

use crossterm::event::KeyEvent;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "recovery";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let mut args = vec!["conversation", "list"];
    if let Some(project_id) = app.project.project_id.as_deref() {
        args.push("--project-id");
        args.push(project_id);
    }
    let value = match invoke_json(globals, &args) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Recovery", error),
    };
    let rows = array_field(&value, "runs")
        .iter()
        .map(|run| {
            let run_id = string_field(run, "runId");
            let status = string_field(run, "status");
            let provider = string_field(run, "providerId");
            let updated = string_field(run, "updatedAt");
            DetailRow {
                title: run_id.clone(),
                subtitle: Some(format!(
                    "{}{}{}",
                    status,
                    if provider.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", provider)
                    },
                    if updated.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", updated)
                    },
                )),
                payload: run.clone(),
            }
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return empty_detail(
            ID,
            "Recovery",
            "No runs yet — recovery tools become available after a first run.",
        );
    }
    rows_detail(
        ID,
        "Recovery",
        Some("rewind/branch/export land in M3 · esc back"),
        rows,
    )
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}
