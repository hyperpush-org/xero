use serde_json::Value as JsonValue;

use super::Diagnostics;

pub(super) fn validate(snapshot: &JsonValue, diagnostics: &mut Diagnostics) {
    super::super::validate_handoff_policy(snapshot.get("handoffPolicy"), diagnostics);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_handoff_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["handoffPolicy"] = json!({"enabled": "yes"});

        let mut diagnostics = Vec::new();
        super::validate(&definition, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_handoff_policy_field_invalid".into()));
    }
}
