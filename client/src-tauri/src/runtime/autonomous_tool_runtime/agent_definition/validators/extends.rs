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
    use std::fs;

    use serde_json::json;

    use crate::{
        db,
        git::repository::CanonicalRepository,
        state::DesktopState,
    };

    #[test]
    fn validates_extends_shape_without_repo() {
        let mut definition = super::super::minimal_definition();
        definition["extends"] = json!("engineer");

        let mut diagnostics = Vec::new();
        super::validate(&definition, "observe_only", None, &mut diagnostics);
        let codes = super::super::diagnostic_codes(&diagnostics);

        assert!(codes.contains(&"agent_definition_extends_invalid".into()));
    }

    #[test]
    fn extends_fixture_covers_shape_cycle_lookup_scope_lifecycle_profile_and_version_boundaries() {
        let mut diagnostics = Vec::new();
        let definition = super::super::minimal_definition();
        super::validate(&definition, "observe_only", None, &mut diagnostics);
        assert!(diagnostics.is_empty(), "missing extends is valid");

        let mut missing_schema = definition.clone();
        missing_schema["extends"] = json!("engineer@1");
        missing_schema
            .as_object_mut()
            .expect("definition object")
            .remove("schemaVersion");
        super::validate(&missing_schema, "observe_only", None, &mut diagnostics);
        assert!(diagnostics.is_empty(), "schema validation owns missing versions");

        let mut unsupported = definition.clone();
        unsupported["extends"] = json!("engineer@1");
        unsupported["schemaVersion"] = json!(0);
        super::validate(&unsupported, "observe_only", None, &mut diagnostics);
        assert_eq!(
            diagnostics.pop().expect("unsupported diagnostic").code,
            "agent_definition_extends_schema_version_unsupported"
        );

        for invalid in [json!(null), json!(" "), json!("engineer"), json!("engineer@0")] {
            let mut invalid_definition = definition.clone();
            invalid_definition["extends"] = invalid;
            super::validate(
                &invalid_definition,
                "observe_only",
                None,
                &mut diagnostics,
            );
            assert_eq!(
                diagnostics.pop().expect("invalid extends diagnostic").code,
                "agent_definition_extends_invalid"
            );
        }

        let mut cycle = definition.clone();
        cycle["id"] = json!("engineer");
        cycle["extends"] = json!("engineer@1");
        super::validate(&cycle, "observe_only", None, &mut diagnostics);
        assert_eq!(
            diagnostics.pop().expect("cycle diagnostic").code,
            "agent_definition_extends_cycle"
        );

        let fixture = tempfile::tempdir().expect("extends fixture");
        let repo_root = fixture.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create extends repository");
        let repo_root = fs::canonicalize(repo_root).expect("canonical extends repository");
        db::configure_project_database_paths(&fixture.path().join("app-data/global.db"));
        db::import_project(
            &CanonicalRepository {
                project_id: "project-extends".into(),
                repository_id: "repository-extends".into(),
                root_path: repo_root.clone(),
                root_path_string: repo_root.to_string_lossy().into_owned(),
                common_git_dir: repo_root.join(".git"),
                display_name: "Extends fixture".into(),
                branch_name: Some("main".into()),
                head_sha: Some("abc123".into()),
                branch: None,
                last_commit: None,
                status_entries: Vec::new(),
                has_staged_changes: false,
                has_unstaged_changes: false,
                has_untracked_changes: false,
                additions: 0,
                deletions: 0,
            },
            DesktopState::default().import_failpoints(),
        )
        .expect("import extends fixture");

        let mut missing = definition.clone();
        missing["extends"] = json!("missing_builtin@1");
        super::validate(
            &missing,
            "observe_only",
            Some(&repo_root),
            &mut diagnostics,
        );
        assert_eq!(
            diagnostics.pop().expect("missing base diagnostic").code,
            "agent_definition_extends_not_found"
        );

        let mut versioned = definition.clone();
        versioned["extends"] = json!("engineer@999");
        super::validate(
            &versioned,
            "observe_only",
            Some(&repo_root),
            &mut diagnostics,
        );
        let codes = super::super::diagnostic_codes(&diagnostics);
        assert!(codes.contains(&"agent_definition_extends_profile_mismatch".into()));
        assert!(codes.contains(&"agent_definition_extends_version_not_found".into()));
        diagnostics.clear();

        let database_path = db::database_path_for_repo(&repo_root);
        let connection = rusqlite::Connection::open(database_path).expect("open extends database");
        connection
            .execute(
                "UPDATE agent_definitions SET scope = 'project_custom' WHERE definition_id = 'engineer'",
                [],
            )
            .expect("make base custom");
        versioned["extends"] = json!("engineer@1");
        super::validate(
            &versioned,
            "observe_only",
            Some(&repo_root),
            &mut diagnostics,
        );
        assert_eq!(
            diagnostics.pop().expect("non-built-in diagnostic").code,
            "agent_definition_extends_not_builtin"
        );

        connection
            .execute(
                "UPDATE agent_definitions SET scope = 'built_in', lifecycle_state = 'archived' WHERE definition_id = 'engineer'",
                [],
            )
            .expect("archive base");
        super::validate(
            &versioned,
            "workspace_write",
            Some(&repo_root),
            &mut diagnostics,
        );
        assert!(super::super::diagnostic_codes(&diagnostics)
            .contains(&"agent_definition_extends_inactive".into()));
    }
}
