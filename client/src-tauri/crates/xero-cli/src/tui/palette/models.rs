//! `model` — list models for the active provider; Enter selects one.

use crossterm::event::{KeyCode, KeyEvent};
use serde_json::json;

use crate::GlobalOptions;

use super::{
    super::app::{invoke_json, App},
    array_field, empty_detail, error_detail, rows_detail, string_field, DetailOutcome, DetailRow,
    DetailState, OpenOutcome,
};

const ID: &str = "model";

pub fn open(globals: &GlobalOptions, app: &mut App) -> OpenOutcome {
    let Some(active_provider_id) = app.selected_provider_id().map(str::to_owned) else {
        return empty_detail(ID, "Models", "No active provider. Use /providers first.");
    };
    if active_provider_id == "fake_provider" {
        return rows_detail(
            ID,
            "Models · fake_provider",
            Some("enter select   esc back"),
            vec![DetailRow {
                title: "fake-model".to_owned(),
                subtitle: Some("selected".to_owned()),
                payload: json!({
                    "providerId": "fake_provider",
                    "modelId": "fake-model",
                    "credentialKind": "",
                }),
            }],
        );
    }

    let value = match invoke_json(globals, &["provider", "list"]) {
        Ok(value) => value,
        Err(error) => return error_detail(ID, "Models", error),
    };
    let Some(provider) = array_field(&value, "providers")
        .iter()
        .find(|provider| string_field(provider, "providerId") == active_provider_id)
    else {
        return empty_detail(
            ID,
            "Models",
            format!("Active provider `{active_provider_id}` is not available."),
        );
    };

    let provider_label = provider_title(provider);
    let credential_kind = string_field(provider, "credentialKind");
    let selected_model_id = app.selected_model_id().unwrap_or_default().to_owned();
    let mut rows = array_field(provider, "models")
        .iter()
        .filter_map(|model| model_row(provider, model, &selected_model_id, &credential_kind))
        .collect::<Vec<_>>();

    if rows.is_empty() {
        let default_model = string_field(provider, "defaultModel");
        if !default_model.trim().is_empty() {
            rows.push(DetailRow {
                title: default_model.clone(),
                subtitle: selected_suffix(&default_model, &selected_model_id),
                payload: json!({
                    "providerId": active_provider_id,
                    "providerLabel": provider_label.clone(),
                    "modelId": default_model,
                    "credentialKind": credential_kind,
                }),
            });
        }
    }

    if rows.is_empty() {
        return empty_detail(
            ID,
            format!("Models · {provider_label}"),
            "No models available for the active provider.",
        );
    }
    rows_detail(
        ID,
        format!("Models · {provider_label}"),
        Some("enter select   esc back"),
        rows,
    )
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
    let provider_label = string_field(&row.payload, "providerLabel");
    let model_id = string_field(&row.payload, "modelId");
    let display_name = string_field(&row.payload, "displayName");
    let credential_kind = string_field(&row.payload, "credentialKind");
    if !app.select_provider_model(provider_id, model_id.clone(), credential_kind) {
        return DetailOutcome::Stay;
    }
    super::super::app::sync_active_session_to_cloud_best_effort(globals, app);

    let model_label = if display_name.trim().is_empty() {
        model_id
    } else {
        display_name
    };
    let status = if provider_label.trim().is_empty() {
        format!("Active model: {model_label}")
    } else {
        format!("Active model: {model_label} ({provider_label})")
    };
    DetailOutcome::Close {
        status: Some(status),
    }
}

fn model_row(
    provider: &serde_json::Value,
    model: &serde_json::Value,
    selected_model_id: &str,
    credential_kind: &str,
) -> Option<DetailRow> {
    let model_id = string_field(model, "modelId");
    if model_id.trim().is_empty() {
        return None;
    }
    let display_name = string_field(model, "displayName");
    let title = if display_name.trim().is_empty() {
        model_id.clone()
    } else {
        display_name.clone()
    };
    let thinking_supported = model
        .get("thinkingSupported")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let subtitle = model_subtitle(&model_id, selected_model_id, thinking_supported);
    Some(DetailRow {
        title,
        subtitle: Some(subtitle),
        payload: json!({
            "providerId": string_field(provider, "providerId"),
            "providerLabel": provider_title(provider),
            "modelId": model_id,
            "displayName": display_name,
            "credentialKind": credential_kind,
        }),
    })
}

fn provider_title(provider: &serde_json::Value) -> String {
    let label = string_field(provider, "label");
    if !label.trim().is_empty() {
        return label;
    }
    string_field(provider, "providerId")
}

fn model_subtitle(model_id: &str, selected_model_id: &str, thinking_supported: bool) -> String {
    let mut parts = vec![model_id.to_owned()];
    if thinking_supported {
        parts.push("thinking".to_owned());
    }
    if selected_model_id == model_id {
        parts.push("selected".to_owned());
    }
    parts.join(" · ")
}

fn selected_suffix(model_id: &str, selected_model_id: &str) -> Option<String> {
    (model_id == selected_model_id).then(|| "selected".to_owned())
}
