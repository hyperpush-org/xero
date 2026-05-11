use serde_json::Value as JsonValue;

use super::Diagnostics;

pub(super) fn validate(snapshot: &JsonValue, diagnostics: &mut Diagnostics) {
    super::super::validate_db_touchpoints_field(snapshot.get("dbTouchpoints"), diagnostics);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_db_touchpoint_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["dbTouchpoints"] = json!({
            "reads": [{"table": "", "purpose": "", "triggers": "not-array", "columns": null}],
            "writes": "not-array",
            "encouraged": []
        });

        let mut diagnostics = Vec::new();
        super::validate(&definition, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_db_touchpoint_text_required".into()));
        assert!(codes.contains(&"agent_definition_db_touchpoint_triggers_required".into()));
        assert!(codes.contains(&"agent_definition_db_touchpoint_columns_required".into()));
        assert!(codes.contains(&"agent_definition_db_touchpoint_array_required".into()));
    }
}
