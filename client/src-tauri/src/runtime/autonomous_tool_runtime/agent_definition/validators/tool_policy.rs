use serde_json::Value as JsonValue;
use std::path::Path;

use super::Diagnostics;

pub(super) fn validate(
    snapshot: &JsonValue,
    base_profile: &str,
    mcp_registry_path: Option<&Path>,
    diagnostics: &mut Diagnostics,
) {
    super::super::validate_tool_policy(
        snapshot.get("toolPolicy"),
        base_profile,
        mcp_registry_path,
        diagnostics,
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_tool_policy_codes_in_isolation() {
        let mut definition = super::super::minimal_definition();
        definition["toolPolicy"]["allowedTools"] = json!(["read", "write"]);

        let mut diagnostics = Vec::new();
        super::validate(&definition, "observe_only", None, &mut diagnostics);
        let denied = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code == "agent_definition_tool_exceeds_profile")
            .expect("denied tool diagnostic");

        assert_eq!(denied.denied_tool.as_deref(), Some("write"));
        assert_eq!(
            denied.base_capability_profile.as_deref(),
            Some("observe_only")
        );
    }
}
