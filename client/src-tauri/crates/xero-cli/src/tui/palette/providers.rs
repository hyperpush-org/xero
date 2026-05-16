//! `providers` — list configured providers; Enter selects one.

use crossterm::event::{KeyCode, KeyEvent};

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App, ProviderRow},
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
    let provider_id = string_field(&row.payload, "providerId");
    let default_model = string_field(&row.payload, "defaultModel");
    let credential_kind = string_field(&row.payload, "credentialKind");
    if provider_id.is_empty() {
        return DetailOutcome::Stay;
    }
    if provider_id == "fake_provider" {
        app.fake_provider_fixture = true;
        return DetailOutcome::Close {
            status: Some("Using fake provider for next run.".to_owned()),
        };
    }
    app.fake_provider_fixture = false;
    // Snap the providers list to match the selection so the composer agent
    // line stays in sync.
    if !app.providers.iter().any(|p| p.provider_id == provider_id) {
        app.providers.push(ProviderRow {
            provider_id: provider_id.clone(),
            default_model: default_model.clone(),
            credential_kind: credential_kind.clone(),
        });
    }
    let index = app
        .providers
        .iter()
        .filter(|p| p.provider_id != "fake_provider")
        .position(|p| p.provider_id == provider_id)
        .unwrap_or(0);
    app.selected_provider = index;
    DetailOutcome::Close {
        status: Some(format!("Active provider: {}", provider_id)),
    }
}
