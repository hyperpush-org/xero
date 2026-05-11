use serde_json::Value as JsonValue;

use super::Diagnostics;

pub(super) fn validate(snapshot: &JsonValue, diagnostics: &mut Diagnostics) {
    super::super::validate_policy_kinds(snapshot.get("projectDataPolicy"), diagnostics);
    super::super::validate_memory_policy(snapshot.get("memoryCandidatePolicy"), diagnostics);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_memory_and_project_policy_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["projectDataPolicy"]["recordKinds"] = json!(["not_a_record_kind"]);
        definition["memoryCandidatePolicy"]["memoryKinds"] = json!(["not_a_memory_kind"]);

        let mut diagnostics = Vec::new();
        super::validate(&definition, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_project_record_kind_unknown".into()));
        assert!(codes.contains(&"agent_definition_memory_kind_unknown".into()));
    }
}
