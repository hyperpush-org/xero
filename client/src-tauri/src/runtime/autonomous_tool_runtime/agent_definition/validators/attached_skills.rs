use std::path::Path;

use serde_json::Value as JsonValue;

use super::Diagnostics;

pub(super) fn validate(
    snapshot: &JsonValue,
    repo_root: Option<&Path>,
    diagnostics: &mut Diagnostics,
) {
    super::super::validate_array_field(snapshot, "attachedSkills", diagnostics);
    super::super::validate_attached_skills(snapshot.get("attachedSkills"), repo_root, diagnostics);
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_attached_skill_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["attachedSkills"] = json!([
            {
                "id": "rust",
                "sourceId": "source-a",
                "skillId": "rust-best-practices",
                "name": "Rust",
                "description": "Rust guidance.",
                "sourceKind": "bundled",
                "scope": "global",
                "versionHash": "hash",
                "includeSupportingAssets": false,
                "required": false
            },
            {
                "id": "rust",
                "sourceId": "source-a",
                "skillId": "rust-best-practices",
                "name": "Rust",
                "description": "Rust guidance.",
                "sourceKind": "bundled",
                "scope": "global",
                "versionHash": "hash-2",
                "includeSupportingAssets": false,
                "required": true
            }
        ]);

        let mut diagnostics = Vec::new();
        super::validate(&definition, None, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_attached_skill_required_flag_invalid".into()));
        assert!(codes.contains(&"agent_definition_attached_skill_duplicate_id".into()));
        assert!(codes.contains(&"agent_definition_attached_skill_duplicate_source_id".into()));
    }
}
