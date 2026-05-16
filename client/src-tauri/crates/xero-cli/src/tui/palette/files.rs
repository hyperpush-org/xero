//! `files` — read-only directory listing rooted at the project root.

use crossterm::event::KeyEvent;
use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "files";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let project_id = match app.project.project_id.as_deref() {
        Some(id) => id,
        None => {
            return empty_detail(
                ID,
                "Files",
                "No project bound. Use `register` to add this directory.",
            );
        }
    };
    let value = match invoke_json(globals, &["file", "list", "--project-id", project_id, "."]) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Files", error),
    };
    let entries = collect_entries(&value);
    if entries.is_empty() {
        return empty_detail(
            ID,
            "Files",
            "Project root is empty (or the tool returned nothing).",
        );
    }
    rows_detail(ID, "Files", Some("esc back"), entries)
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}

fn collect_entries(value: &JsonValue) -> Vec<DetailRow> {
    // The file tool returns its payload under varying keys depending on the
    // execution path. Probe a couple of common shapes.
    if let Some(entries) = value
        .get("entries")
        .or_else(|| value.get("files"))
        .and_then(JsonValue::as_array)
    {
        return entries
            .iter()
            .map(|entry| {
                let name = string_field(entry, "name");
                let path = string_field(entry, "path");
                let kind = string_field(entry, "kind");
                DetailRow {
                    title: if !name.is_empty() {
                        name.clone()
                    } else {
                        path.clone()
                    },
                    subtitle: if kind.is_empty() { None } else { Some(kind) },
                    payload: entry.clone(),
                }
            })
            .collect();
    }
    if let Some(text) = value
        .get("output")
        .and_then(JsonValue::as_str)
        .or_else(|| value.get("text").and_then(JsonValue::as_str))
    {
        return text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| DetailRow {
                title: line.to_owned(),
                subtitle: None,
                payload: JsonValue::Null,
            })
            .collect();
    }
    array_field(value, "items")
        .iter()
        .map(|item| DetailRow {
            title: string_field(item, "name"),
            subtitle: None,
            payload: item.clone(),
        })
        .collect()
}
