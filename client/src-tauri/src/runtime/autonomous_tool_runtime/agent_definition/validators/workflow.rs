use serde_json::Value as JsonValue;

use super::Diagnostics;

pub(super) fn validate(snapshot: &JsonValue, diagnostics: &mut Diagnostics) {
    super::super::validate_required_contract_text(snapshot, "workflowContract", diagnostics);
    super::super::validate_workflow_structure(snapshot.get("workflowStructure"), diagnostics);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_workflow_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["workflowContract"] = json!("");
        definition["workflowStructure"] = json!({
            "startPhaseId": "missing",
            "phases": [
                {
                    "id": "first",
                    "title": "First",
                    "allowedTools": ["not_a_tool"],
                    "branches": [{"targetPhaseId": "missing", "condition": {"kind": "always"}}]
                }
            ]
        });

        let mut diagnostics = Vec::new();
        super::validate(&definition, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_contract_required".into()));
        assert!(codes.contains(&"agent_definition_workflow_start_phase_unknown".into()));
        assert!(codes.contains(&"agent_definition_workflow_tool_unknown".into()));
        assert!(codes.contains(&"agent_definition_workflow_branch_target_unknown".into()));
    }
}
