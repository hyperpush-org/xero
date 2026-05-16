//! `usage` — tokens & cost summary.

use crossterm::event::KeyEvent;
use serde_json::Value as JsonValue;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "usage";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let mut args = vec!["usage", "summary"];
    if let Some(project_id) = app.project.project_id.as_deref() {
        args.push("--project-id");
        args.push(project_id);
    }
    let value = match invoke_json(globals, &args) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Usage", error),
    };
    let rows = array_field(&value, "rows")
        .iter()
        .map(|row| {
            let provider = string_field(row, "providerId");
            let model = string_field(row, "modelId");
            let runs = number_field(row, "runCount");
            let input = number_field(row, "inputTokens");
            let output = number_field(row, "outputTokens");
            let total = number_field(row, "totalTokens");
            let cost_micros = number_field(row, "estimatedCostMicros");
            let cost_label = if cost_micros == 0 {
                String::new()
            } else {
                format!(" · ${:.4}", (cost_micros as f64) / 1_000_000.0)
            };
            DetailRow {
                title: format!("{} · {}", provider, model),
                subtitle: Some(format!(
                    "{} runs · in {} / out {} / total {}{}",
                    runs, input, output, total, cost_label
                )),
                payload: row.clone(),
            }
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return empty_detail(ID, "Usage", "No usage recorded yet.");
    }
    rows_detail(ID, "Usage", Some("esc back"), rows)
}

pub fn handle_key(
    _app: &mut App,
    _detail: &mut DetailState,
    _key: KeyEvent,
    _globals: &GlobalOptions,
) -> DetailOutcome {
    DetailOutcome::Stay
}

fn number_field(value: &JsonValue, key: &str) -> u64 {
    value
        .get(key)
        .and_then(JsonValue::as_u64)
        .unwrap_or_default()
}
