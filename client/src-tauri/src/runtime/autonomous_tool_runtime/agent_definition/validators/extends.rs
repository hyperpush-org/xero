use std::path::Path;

use serde_json::Value as JsonValue;

use crate::db::project_store;

use super::Diagnostics;

pub(super) fn validate(
    snapshot: &JsonValue,
    base_profile: &str,
    repo_root: Option<&Path>,
    diagnostics: &mut Diagnostics,
) {
    let Some(extends) = snapshot.get("extends") else {
        return;
    };
    let Some(schema_version) = snapshot.get("schemaVersion").and_then(JsonValue::as_u64) else {
        return;
    };
    if schema_version != super::super::AGENT_DEFINITION_SCHEMA_VERSION {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_extends_schema_version_unsupported",
            format!(
                "extends is only supported by schemaVersion {}.",
                super::super::AGENT_DEFINITION_SCHEMA_VERSION
            ),
            "extends",
        ));
        return;
    }

    let Some(extends_text) = extends
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_extends_invalid",
            "extends must be a non-empty built-in reference like engineer@1.",
            "extends",
        ));
        return;
    };
    let spec = match project_store::parse_agent_definition_extends_spec(extends_text) {
        Ok(spec) => spec,
        Err(message) => {
            diagnostics.push(super::super::diagnostic(
                "agent_definition_extends_invalid",
                message,
                "extends",
            ));
            return;
        }
    };

    if snapshot
        .get("id")
        .and_then(JsonValue::as_str)
        .is_some_and(|definition_id| definition_id == spec.base_definition_id)
    {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_extends_cycle",
            "A custom agent definition cannot extend itself.",
            "extends",
        ));
    }

    let Some(repo_root) = repo_root else {
        return;
    };
    let base_definition =
        match project_store::load_agent_definition(repo_root, &spec.base_definition_id) {
            Ok(Some(definition)) => definition,
            Ok(None) => {
                diagnostics.push(super::super::diagnostic(
                    "agent_definition_extends_not_found",
                    format!(
                        "Built-in agent definition `{}` was not found.",
                        spec.base_definition_id
                    ),
                    "extends",
                ));
                return;
            }
            Err(error) => {
                diagnostics.push(super::super::diagnostic(
                    "agent_definition_extends_lookup_failed",
                    error.message,
                    "extends",
                ));
                return;
            }
        };
    if base_definition.scope != "built_in" {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_extends_not_builtin",
            format!(
                "extends must reference a built-in agent definition; `{}` is `{}`.",
                base_definition.definition_id, base_definition.scope
            ),
            "extends",
        ));
        return;
    }
    if base_definition.lifecycle_state != "active" {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_extends_inactive",
            format!(
                "Built-in agent definition `{}` is `{}`.",
                base_definition.definition_id, base_definition.lifecycle_state
            ),
            "extends",
        ));
    }
    if base_definition.base_capability_profile != base_profile {
        diagnostics.push(super::super::diagnostic(
            "agent_definition_extends_profile_mismatch",
            format!(
                "extends `{}` uses base capability profile `{}`; overlay declares `{}`.",
                extends_text, base_definition.base_capability_profile, base_profile
            ),
            "baseCapabilityProfile",
        ));
    }

    match project_store::load_agent_definition_version(
        repo_root,
        &spec.base_definition_id,
        spec.base_version,
    ) {
        Ok(Some(_)) => {}
        Ok(None) => diagnostics.push(super::super::diagnostic(
            "agent_definition_extends_version_not_found",
            format!(
                "Built-in agent definition `{}` version {} was not found.",
                spec.base_definition_id, spec.base_version
            ),
            "extends",
        )),
        Err(error) => diagnostics.push(super::super::diagnostic(
            "agent_definition_extends_lookup_failed",
            error.message,
            "extends",
        )),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    #[test]
    fn validates_extends_shape_without_repo() {
        let mut definition = super::super::minimal_definition();
        definition["extends"] = json!("engineer");

        let mut diagnostics = Vec::new();
        super::validate(&definition, "observe_only", None, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_extends_invalid".into()));
    }
}
