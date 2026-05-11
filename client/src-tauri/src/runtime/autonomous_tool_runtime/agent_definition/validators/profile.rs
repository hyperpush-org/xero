use serde_json::Value as JsonValue;

use super::Diagnostics;

pub(super) fn validate(snapshot: &JsonValue, diagnostics: &mut Diagnostics) -> String {
    let base_profile =
        super::super::snapshot_text(snapshot, "baseCapabilityProfile").unwrap_or_default();
    if ![
        "observe_only",
        "planning",
        "repository_recon",
        "engineering",
        "debugging",
        "agent_builder",
    ]
    .contains(&base_profile.as_str())
    {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_base_profile_invalid",
            "Base capability profile must be observe_only, planning, repository_recon, engineering, debugging, or agent_builder.",
            "baseCapabilityProfile",
        ));
    }

    super::super::validate_approval_modes(snapshot, &base_profile, diagnostics);
    base_profile
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_profile_and_approval_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["baseCapabilityProfile"] = json!("observe_only");
        definition["defaultApprovalMode"] = json!("yolo");
        definition["allowedApprovalModes"] = json!(["suggest", "yolo"]);

        let mut diagnostics = Vec::new();
        let base_profile = super::validate(&definition, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert_eq!(base_profile, "observe_only");
        assert!(codes.contains(&"agent_definition_approval_exceeds_profile".into()));
    }
}
