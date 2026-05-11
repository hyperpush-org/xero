use serde_json::Value as JsonValue;

use super::Diagnostics;

pub(super) fn validate(snapshot: &JsonValue, diagnostics: &mut Diagnostics) {
    super::super::validate_array_field(snapshot, "prompts", diagnostics);
    super::super::validate_array_field(snapshot, "tools", diagnostics);
    super::super::validate_array_field(snapshot, "consumes", diagnostics);
    super::super::validate_prompt_intent(snapshot, diagnostics);
    super::super::validate_output_field(snapshot.get("output"), diagnostics);
    super::super::validate_required_contract_text(snapshot, "finalResponseContract", diagnostics);
    super::super::validate_examples(
        snapshot.get("examplePrompts"),
        "examplePrompts",
        diagnostics,
    );
    super::super::validate_examples(
        snapshot.get("refusalEscalationCases"),
        "refusalEscalationCases",
        diagnostics,
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_output_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["prompts"] = json!([]);
        definition["promptFragments"] = json!({});
        definition["output"]["contract"] = json!("unsupported_contract");
        definition["output"]["sections"] = json!([]);
        definition["examplePrompts"] = json!(["only one"]);

        let mut diagnostics = Vec::new();
        super::validate(&definition, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_prompt_intent_missing".into()));
        assert!(codes.contains(&"agent_definition_output_contract_unknown".into()));
        assert!(codes.contains(&"agent_definition_output_sections_required".into()));
        assert!(codes.contains(&"agent_definition_examples_required".into()));
    }
}
