use serde_json::Value as JsonValue;

use super::Diagnostics;

pub(super) fn validate(snapshot: &JsonValue, diagnostics: &mut Diagnostics) {
    let object = snapshot.as_object();
    super::super::validate_schema_metadata(snapshot, diagnostics);
    super::super::validate_text_field(
        object,
        "id",
        super::super::MAX_DEFINITION_ID_CHARS,
        diagnostics,
    );
    super::super::validate_text_field(
        object,
        "displayName",
        super::super::MAX_DISPLAY_NAME_CHARS,
        diagnostics,
    );
    super::super::validate_text_field(
        object,
        "shortLabel",
        super::super::MAX_SHORT_LABEL_CHARS,
        diagnostics,
    );
    super::super::validate_text_field(
        object,
        "description",
        super::super::MAX_DESCRIPTION_CHARS,
        diagnostics,
    );
    super::super::validate_text_field(
        object,
        "taskPurpose",
        super::super::MAX_DESCRIPTION_CHARS,
        diagnostics,
    );

    let scope = super::super::snapshot_text(snapshot, "scope").unwrap_or_default();
    if !["global_custom", "project_custom"].contains(&scope.as_str()) {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_scope_invalid",
            "Custom agent definitions saved by Agent Create must be global_custom or project_custom.",
            "scope",
        ));
    }
    let lifecycle_state =
        super::super::snapshot_text(snapshot, "lifecycleState").unwrap_or_default();
    if !["draft", "valid", "active", "archived", "blocked"].contains(&lifecycle_state.as_str()) {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_lifecycle_invalid",
            "Lifecycle state must be draft, valid, active, archived, or blocked.",
            "lifecycleState",
        ));
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_identity_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["schema"] = json!("wrong.schema");
        definition["id"] = json!("");
        definition["scope"] = json!("built_in");
        definition["lifecycleState"] = json!("unknown");

        let mut diagnostics = Vec::new();
        super::validate(&definition, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_schema_unsupported".into()));
        assert!(codes.contains(&"agent_definition_text_required".into()));
        assert!(codes.contains(&"agent_definition_scope_invalid".into()));
        assert!(codes.contains(&"agent_definition_lifecycle_invalid".into()));
    }
}
