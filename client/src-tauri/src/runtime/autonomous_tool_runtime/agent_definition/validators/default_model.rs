use serde_json::Value as JsonValue;

use super::Diagnostics;
use crate::runtime::autonomous_tool_runtime::agent_definition::diagnostic;

pub(super) fn validate(value: Option<&JsonValue>, diagnostics: &mut Diagnostics) {
    let Some(value) = value else {
        return;
    };
    let Some(object) = value.as_object() else {
        diagnostics.push(diagnostic(
            "agent_definition_default_model_invalid",
            "defaultModel must be an object when provided.",
            "defaultModel",
        ));
        return;
    };

    validate_required_string(
        object.get("providerId"),
        "agent_definition_default_model_provider_required",
        "defaultModel.providerId must be a non-empty string.",
        "defaultModel.providerId",
        diagnostics,
    );
    validate_required_string(
        object.get("modelId"),
        "agent_definition_default_model_model_required",
        "defaultModel.modelId must be a non-empty string.",
        "defaultModel.modelId",
        diagnostics,
    );

    if let Some(provider_profile_id) = object.get("providerProfileId") {
        validate_optional_string(
            provider_profile_id,
            "agent_definition_default_model_profile_invalid",
            "defaultModel.providerProfileId must be null or a non-empty string.",
            "defaultModel.providerProfileId",
            diagnostics,
        );
    }
    if let Some(selection_key) = object.get("selectionKey") {
        validate_optional_string(
            selection_key,
            "agent_definition_default_model_selection_key_invalid",
            "defaultModel.selectionKey must be a non-empty string when provided.",
            "defaultModel.selectionKey",
            diagnostics,
        );
    }
    if let Some(thinking_effort) = object.get("thinkingEffort") {
        let valid = thinking_effort.is_null()
            || matches!(
                thinking_effort.as_str(),
                Some("none" | "minimal" | "low" | "medium" | "high" | "x_high")
            );
        if !valid {
            diagnostics.push(diagnostic(
                "agent_definition_default_model_thinking_effort_invalid",
                "defaultModel.thinkingEffort must be null or a supported thinking effort.",
                "defaultModel.thinkingEffort",
            ));
        }
    }

    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "providerId" | "providerProfileId" | "modelId" | "selectionKey" | "thinkingEffort"
        ) {
            diagnostics.push(diagnostic(
                "agent_definition_default_model_unknown_field",
                format!("defaultModel contains unsupported field `{key}`."),
                format!("defaultModel.{key}"),
            ));
        }
    }
}

fn validate_required_string(
    value: Option<&JsonValue>,
    code: &str,
    message: &str,
    path: &str,
    diagnostics: &mut Diagnostics,
) {
    match value.and_then(JsonValue::as_str).map(str::trim) {
        Some(value) if !value.is_empty() => {}
        _ => diagnostics.push(diagnostic(code, message, path)),
    }
}

fn validate_optional_string(
    value: &JsonValue,
    code: &str,
    message: &str,
    path: &str,
    diagnostics: &mut Diagnostics,
) {
    if value.is_null() {
        return;
    }
    match value.as_str().map(str::trim) {
        Some(value) if !value.is_empty() => {}
        _ => diagnostics.push(diagnostic(code, message, path)),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn accepts_valid_default_model() {
        let mut definition = super::super::minimal_definition();
        definition["defaultModel"] = json!({
            "providerId": "anthropic",
            "providerProfileId": "anthropic-work",
            "modelId": "claude-sonnet-4-5",
            "selectionKey": "anthropic:claude-sonnet-4-5",
            "thinkingEffort": "medium"
        });

        let report =
            super::super::validate_definition_snapshot_with_registry(&definition, None, None);

        assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    }

    #[test]
    fn rejects_malformed_default_model() {
        let mut definition = super::super::minimal_definition();
        definition["defaultModel"] = json!({
            "providerId": "",
            "modelId": " ",
            "thinkingEffort": "maximum",
            "extra": true
        });

        let report =
            super::super::validate_definition_snapshot_with_registry(&definition, None, None);
        let codes = super::super::diagnostic_codes(&report.diagnostics);

        assert!(codes.contains(&"agent_definition_default_model_provider_required".into()));
        assert!(codes.contains(&"agent_definition_default_model_model_required".into()));
        assert!(codes.contains(&"agent_definition_default_model_thinking_effort_invalid".into()));
        assert!(codes.contains(&"agent_definition_default_model_unknown_field".into()));
    }
}
