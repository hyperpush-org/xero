//! `agents` — list agent definitions.

use crossterm::event::KeyEvent;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "agents";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let mut args = vec!["agent-definition", "list"];
    if let Some(project_id) = app.project.project_id.as_deref() {
        args.push("--project-id");
        args.push(project_id);
    }
    let value = match invoke_json(globals, &args) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Agent definitions", error),
    };
    let rows = array_field(&value, "definitions")
        .iter()
        .map(|definition| {
            let display_name = string_field(definition, "displayName");
            let definition_id = string_field(definition, "definitionId");
            let current_version = string_field(definition, "currentVersion");
            let lifecycle = string_field(definition, "lifecycleState");
            DetailRow {
                title: if display_name.is_empty() {
                    definition_id.clone()
                } else {
                    display_name
                },
                subtitle: Some(format!(
                    "{}{}{}",
                    definition_id,
                    if current_version.is_empty() {
                        String::new()
                    } else {
                        format!(" · v{}", current_version)
                    },
                    if lifecycle.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", lifecycle)
                    }
                )),
                payload: definition.clone(),
            }
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return empty_detail(
            ID,
            "Agent definitions",
            "No agent definitions registered for this scope.",
        );
    }
    rows_detail(ID, "Agent definitions", Some("esc back"), rows)
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}
