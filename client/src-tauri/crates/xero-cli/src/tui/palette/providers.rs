//! `providers` — list configured providers; Enter selects one.

use crossterm::event::{KeyCode, KeyEvent};

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "providers";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let value = match invoke_json(globals, &["provider", "list"]) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Providers", error),
    };
    let mut rows = Vec::new();
    let selected_id = app.selected_provider_id().map(str::to_owned);
    for provider in array_field(&value, "providers") {
        let provider_id = string_field(provider, "providerId");
        let label = string_field(provider, "label");
        let default_model = string_field(provider, "defaultModel");
        let status = string_field(provider, "headlessStatus");
        let selected = selected_id.as_deref() == Some(provider_id.as_str());
        let title = if label.is_empty() {
            provider_id.clone()
        } else {
            format!("{} ({})", label, provider_id)
        };
        let marker = if selected { " · selected" } else { "" };
        rows.push(DetailRow {
            title,
            subtitle: Some(format!(
                "{}{}{}",
                default_model,
                if status.is_empty() {
                    String::new()
                } else {
                    format!(" · {}", status)
                },
                marker,
            )),
            payload: provider.clone(),
        });
    }
    if rows.is_empty() {
        return empty_detail(ID, "Providers", "No providers configured.");
    }
    rows_detail(ID, "Providers", Some("enter select   esc back"), rows)
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
    let super::DetailData::Rows(rows) = &detail.data else {
        return DetailOutcome::Stay;
    };
    let Some(row) = rows.get(detail.selected) else {
        return DetailOutcome::Stay;
    };
    let provider_id = string_field(&row.payload, "providerId");
    let default_model = string_field(&row.payload, "defaultModel");
    let credential_kind = string_field(&row.payload, "credentialKind");
    if provider_id.is_empty() {
        return DetailOutcome::Stay;
    }
    if provider_id == "fake_provider" {
        app.select_provider_model("fake_provider", "fake-model", "");
        super::super::app::sync_active_session_to_cloud_best_effort(globals, app);
        return DetailOutcome::Close {
            status: Some("Using fake provider for next run.".to_owned()),
        };
    }
    app.select_provider_model(provider_id.clone(), default_model, credential_kind);
    super::super::app::sync_active_session_to_cloud_best_effort(globals, app);
    DetailOutcome::Close {
        status: Some(format!("Active provider: {}", provider_id)),
    }
}
