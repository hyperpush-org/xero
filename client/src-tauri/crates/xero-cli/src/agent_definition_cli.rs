use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use super::{
    now_timestamp, project_cli, response, take_bool_flag, take_help, take_option, CliError,
    CliResponse, GlobalOptions,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentDefinitionRow {
    definition_id: String,
    current_version: i64,
    display_name: String,
    short_label: String,
    scope: String,
    lifecycle_state: String,
    base_capability_profile: String,
    updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentDefinitionVersionRow {
    definition_id: String,
    version: i64,
    snapshot: JsonValue,
    validation_report: Option<JsonValue>,
    created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct AgentDefinitionVersionSummaryRow {
    definition_id: String,
    version: i64,
    validation_status: String,
    created_at: String,
}

pub(crate) fn dispatch_agent_definition(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") => command_agent_definition_list(globals, args[1..].to_vec()),
        Some("show") => command_agent_definition_show(globals, args[1..].to_vec()),
        Some("versions") => command_agent_definition_versions(globals, args[1..].to_vec()),
        Some("diff") => command_agent_definition_diff(globals, args[1..].to_vec()),
        Some("archive") => command_agent_definition_archive(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero agent-definition list|show|versions|diff|archive --project-id ID\nBrowses and manages saved agent-definition records. Save/validate/Stage editing stay on the shared autonomous definition service until it is exposed to terminal clients.",
            json!({ "command": "agent-definition" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown agent-definition command `{other}`. Use list, show, versions, diff, or archive."
        ))),
    }
}

fn command_agent_definition_list(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero agent-definition list [--project-id ID] [--include-archived]",
            json!({ "command": "agent-definition list" }),
        ));
    }
    let include_archived = take_bool_flag(&mut args, "--include-archived");
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    reject_agent_definition_unknown_options(&args)?;
    let definitions = list_agent_definitions(&globals, &project_id, include_archived)?;
    let text = if definitions.is_empty() {
        "No agent definitions found.".into()
    } else {
        definitions
            .iter()
            .map(|definition| {
                format!(
                    "{:<24} v{:<3} {:<12} {}",
                    definition.definition_id,
                    definition.current_version,
                    definition.lifecycle_state,
                    definition.display_name
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "agentDefinitionList",
            "projectId": project_id,
            "includeArchived": include_archived,
            "definitions": definitions
        }),
    ))
}

fn command_agent_definition_diff(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero agent-definition diff DEFINITION_ID --from N --to N [--project-id ID]",
            json!({ "command": "agent-definition diff" }),
        ));
    }
    let from_version = take_option(&mut args, "--from")?
        .map(|value| parse_version(&value))
        .transpose()?
        .ok_or_else(|| CliError::usage("Missing `--from` version."))?;
    let to_version = take_option(&mut args, "--to")?
        .map(|value| parse_version(&value))
        .transpose()?
        .ok_or_else(|| CliError::usage("Missing `--to` version."))?;
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let definition_id = args
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage("Missing definition id."))?;
    reject_agent_definition_unknown_options(&args[1..])?;
    let diff = agent_definition_version_diff(
        &globals,
        &project_id,
        &definition_id,
        from_version,
        to_version,
    )?;
    let changed_sections = diff["changedSections"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(JsonValue::as_str)
        .collect::<Vec<_>>();
    let text = if changed_sections.is_empty() {
        format!("No changes between `{definition_id}` v{from_version} and v{to_version}.")
    } else {
        format!(
            "{} changed section(s): {}",
            changed_sections.len(),
            changed_sections.join(", ")
        )
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "agentDefinitionDiff",
            "projectId": project_id,
            "definitionId": definition_id,
            "diff": diff
        }),
    ))
}

fn command_agent_definition_archive(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero agent-definition archive DEFINITION_ID [--project-id ID]",
            json!({ "command": "agent-definition archive" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let definition_id = args
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage("Missing definition id."))?;
    reject_agent_definition_unknown_options(&args[1..])?;
    let archived = archive_agent_definition(&globals, &project_id, &definition_id)?;
    Ok(response(
        &globals,
        format!(
            "Archived agent definition `{}` for project `{}`.",
            archived.definition_id, project_id
        ),
        json!({
            "kind": "agentDefinitionArchive",
            "projectId": project_id,
            "definition": archived
        }),
    ))
}

fn command_agent_definition_show(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero agent-definition show DEFINITION_ID [--version N] [--project-id ID]",
            json!({ "command": "agent-definition show" }),
        ));
    }
    let version = take_option(&mut args, "--version")?
        .map(|value| parse_version(&value))
        .transpose()?;
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let definition_id = args
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage("Missing definition id."))?;
    reject_agent_definition_unknown_options(&args[1..])?;
    let definition = agent_definition_version(&globals, &project_id, &definition_id, version)?;
    let text = serde_json::to_string_pretty(&definition.snapshot).map_err(|error| {
        CliError::system_fault(
            "xero_cli_agent_definition_encode_failed",
            format!("Could not encode definition snapshot: {error}"),
        )
    })?;
    Ok(response(
        &globals,
        text,
        json!({ "kind": "agentDefinitionShow", "projectId": project_id, "definition": definition }),
    ))
}

fn command_agent_definition_versions(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero agent-definition versions DEFINITION_ID [--project-id ID]",
            json!({ "command": "agent-definition versions" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    let definition_id = args
        .first()
        .cloned()
        .ok_or_else(|| CliError::usage("Missing definition id."))?;
    reject_agent_definition_unknown_options(&args[1..])?;
    let versions = list_agent_definition_versions(&globals, &project_id, &definition_id)?;
    let text = if versions.is_empty() {
        format!("No saved versions found for `{definition_id}`.")
    } else {
        versions
            .iter()
            .map(|version| {
                format!(
                    "{} v{} validation={} created={}",
                    version.definition_id,
                    version.version,
                    version.validation_status,
                    version.created_at
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "kind": "agentDefinitionVersions",
            "projectId": project_id,
            "definitionId": definition_id,
            "versions": versions
        }),
    ))
}

fn list_agent_definitions(
    globals: &GlobalOptions,
    project_id: &str,
    include_archived: bool,
) -> Result<Vec<AgentDefinitionRow>, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    let sql = if include_archived {
        r#"
            SELECT definition_id, current_version, display_name, short_label, scope,
                   lifecycle_state, base_capability_profile, updated_at
            FROM agent_definitions
            ORDER BY
                CASE scope
                    WHEN 'built_in' THEN 0
                    WHEN 'project_custom' THEN 1
                    ELSE 2
                END,
                display_name COLLATE NOCASE,
                definition_id COLLATE NOCASE
            "#
    } else {
        r#"
            SELECT definition_id, current_version, display_name, short_label, scope,
                   lifecycle_state, base_capability_profile, updated_at
            FROM agent_definitions
            WHERE lifecycle_state = 'active'
            ORDER BY
                CASE scope
                    WHEN 'built_in' THEN 0
                    WHEN 'project_custom' THEN 1
                    ELSE 2
                END,
                display_name COLLATE NOCASE,
                definition_id COLLATE NOCASE
            "#
    };
    let mut statement = connection
        .prepare(sql)
        .map_err(|error| sqlite_agent_definition_error("prepare", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok(AgentDefinitionRow {
                definition_id: row.get(0)?,
                current_version: row.get(1)?,
                display_name: row.get(2)?,
                short_label: row.get(3)?,
                scope: row.get(4)?,
                lifecycle_state: row.get(5)?,
                base_capability_profile: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .map_err(|error| sqlite_agent_definition_error("query", error))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| sqlite_agent_definition_error("decode", error))
}

fn list_agent_definition_versions(
    globals: &GlobalOptions,
    project_id: &str,
    definition_id: &str,
) -> Result<Vec<AgentDefinitionVersionSummaryRow>, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT definition_id, version, validation_report_json, created_at
            FROM agent_definition_versions
            WHERE definition_id = ?1
            ORDER BY version DESC
            "#,
        )
        .map_err(|error| sqlite_agent_definition_error("versions_prepare", error))?;
    let rows = statement
        .query_map(params![definition_id], |row| {
            let validation_json = row.get::<_, Option<String>>(2)?;
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                validation_json,
                row.get::<_, String>(3)?,
            ))
        })
        .map_err(|error| sqlite_agent_definition_error("versions_query", error))?;
    rows.map(|row| {
        let (definition_id, version, validation_json, created_at) =
            row.map_err(|error| sqlite_agent_definition_error("versions_decode", error))?;
        let validation_status = validation_json
            .as_deref()
            .and_then(|value| parse_json_column("validation_report_json", value).ok())
            .and_then(|value| {
                value
                    .get("status")
                    .or_else(|| value.get("state"))
                    .and_then(JsonValue::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_else(|| "unknown".into());
        Ok(AgentDefinitionVersionSummaryRow {
            definition_id,
            version,
            validation_status,
            created_at,
        })
    })
    .collect()
}

fn agent_definition_version(
    globals: &GlobalOptions,
    project_id: &str,
    definition_id: &str,
    version: Option<i64>,
) -> Result<AgentDefinitionVersionRow, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    let resolved_version = match version {
        Some(version) => version,
        None => connection
            .query_row(
                "SELECT current_version FROM agent_definitions WHERE definition_id = ?1",
                params![definition_id],
                |row| row.get::<_, i64>(0),
            )
            .optional()
            .map_err(|error| sqlite_agent_definition_error("current_version", error))?
            .ok_or_else(|| {
                CliError::user_fixable(
                    "xero_cli_agent_definition_unknown",
                    format!("Agent definition `{definition_id}` was not found."),
                )
            })?,
    };
    connection
        .query_row(
            r#"
            SELECT definition_id, version, snapshot_json, validation_report_json, created_at
            FROM agent_definition_versions
            WHERE definition_id = ?1 AND version = ?2
            "#,
            params![definition_id, resolved_version],
            |row| {
                let snapshot_json: String = row.get(2)?;
                let validation_json: Option<String> = row.get(3)?;
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    snapshot_json,
                    validation_json,
                    row.get(4)?,
                ))
            },
        )
        .optional()
        .map_err(|error| sqlite_agent_definition_error("version", error))?
        .map(
            |(definition_id, version, snapshot_json, validation_json, created_at)| {
                Ok(AgentDefinitionVersionRow {
                    definition_id,
                    version,
                    snapshot: parse_json_column("snapshot_json", &snapshot_json)?,
                    validation_report: validation_json
                        .as_deref()
                        .map(|value| parse_json_column("validation_report_json", value))
                        .transpose()?,
                    created_at,
                })
            },
        )
        .transpose()?
        .ok_or_else(|| {
            CliError::user_fixable(
                "xero_cli_agent_definition_version_unknown",
                format!(
                    "Agent definition `{definition_id}` version {resolved_version} was not found."
                ),
            )
        })
}

fn agent_definition_version_diff(
    globals: &GlobalOptions,
    project_id: &str,
    definition_id: &str,
    from_version: i64,
    to_version: i64,
) -> Result<JsonValue, CliError> {
    if from_version == to_version {
        return Err(CliError::usage(
            "`--from` and `--to` must be different versions.",
        ));
    }
    let from = agent_definition_version(globals, project_id, definition_id, Some(from_version))?;
    let to = agent_definition_version(globals, project_id, definition_id, Some(to_version))?;
    Ok(build_agent_definition_diff_payload(
        definition_id,
        from.version,
        &from.created_at,
        &from.snapshot,
        to.version,
        &to.created_at,
        &to.snapshot,
    ))
}

fn archive_agent_definition(
    globals: &GlobalOptions,
    project_id: &str,
    definition_id: &str,
) -> Result<AgentDefinitionRow, CliError> {
    let connection = project_cli::project_connection(globals, project_id)?;
    let definition = load_agent_definition_row(&connection, definition_id)?.ok_or_else(|| {
        CliError::user_fixable(
            "xero_cli_agent_definition_unknown",
            format!("Agent definition `{definition_id}` was not found."),
        )
    })?;
    if definition.scope == "built_in" {
        return Err(CliError::user_fixable(
            "xero_cli_agent_definition_builtin_immutable",
            format!("Built-in agent definition `{definition_id}` cannot be archived."),
        ));
    }
    let updated_at = now_timestamp();
    connection
        .execute(
            "UPDATE agent_definitions SET lifecycle_state = 'archived', updated_at = ?2 WHERE definition_id = ?1",
            params![definition_id, updated_at],
        )
        .map_err(|error| sqlite_agent_definition_error("archive", error))?;
    load_agent_definition_row(&connection, definition_id)?.ok_or_else(|| {
        CliError::system_fault(
            "xero_cli_agent_definition_archive_missing",
            "Agent definition archive succeeded but the row could not be loaded back.",
        )
    })
}

fn load_agent_definition_row(
    connection: &rusqlite::Connection,
    definition_id: &str,
) -> Result<Option<AgentDefinitionRow>, CliError> {
    connection
        .query_row(
            r#"
            SELECT definition_id, current_version, display_name, short_label, scope,
                   lifecycle_state, base_capability_profile, updated_at
            FROM agent_definitions
            WHERE definition_id = ?1
            "#,
            params![definition_id],
            |row| {
                Ok(AgentDefinitionRow {
                    definition_id: row.get(0)?,
                    current_version: row.get(1)?,
                    display_name: row.get(2)?,
                    short_label: row.get(3)?,
                    scope: row.get(4)?,
                    lifecycle_state: row.get(5)?,
                    base_capability_profile: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .optional()
        .map_err(|error| sqlite_agent_definition_error("load", error))
}

fn build_agent_definition_diff_payload(
    definition_id: &str,
    from_version: i64,
    from_created_at: &str,
    from_snapshot: &JsonValue,
    to_version: i64,
    to_created_at: &str,
    to_snapshot: &JsonValue,
) -> JsonValue {
    let sections = diff_sections(from_snapshot, to_snapshot);
    let changed_sections = sections
        .iter()
        .filter(|section| section["changed"].as_bool() == Some(true))
        .filter_map(|section| section["section"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();

    json!({
        "schema": "xero.agent_definition_version_diff.v1",
        "definitionId": definition_id,
        "fromVersion": from_version,
        "toVersion": to_version,
        "fromCreatedAt": from_created_at,
        "toCreatedAt": to_created_at,
        "changed": !changed_sections.is_empty(),
        "changedSections": changed_sections,
        "sections": sections,
    })
}

fn diff_sections(from_snapshot: &JsonValue, to_snapshot: &JsonValue) -> [JsonValue; 12] {
    [
        diff_section(
            "identity",
            from_snapshot,
            to_snapshot,
            &[
                "id",
                "displayName",
                "shortLabel",
                "description",
                "taskPurpose",
                "scope",
                "lifecycleState",
                "baseCapabilityProfile",
            ],
        ),
        diff_section(
            "prompts",
            from_snapshot,
            to_snapshot,
            &[
                "promptPolicy",
                "prompts",
                "promptFragments",
                "workflowContract",
                "finalResponseContract",
                "examples",
                "escalationCases",
            ],
        ),
        diff_section(
            "attachedSkills",
            from_snapshot,
            to_snapshot,
            &["attachedSkills"],
        ),
        diff_section(
            "toolPolicy",
            from_snapshot,
            to_snapshot,
            &["toolPolicy", "tools"],
        ),
        diff_section(
            "memoryPolicy",
            from_snapshot,
            to_snapshot,
            &["memoryPolicy"],
        ),
        diff_section(
            "retrievalPolicy",
            from_snapshot,
            to_snapshot,
            &["retrievalDefaults"],
        ),
        diff_section(
            "handoffPolicy",
            from_snapshot,
            to_snapshot,
            &["handoffPolicy"],
        ),
        diff_section(
            "outputContract",
            from_snapshot,
            to_snapshot,
            &["outputContract", "output"],
        ),
        diff_section(
            "databaseAccess",
            from_snapshot,
            to_snapshot,
            &["dbTouchpoints"],
        ),
        diff_section(
            "consumedArtifacts",
            from_snapshot,
            to_snapshot,
            &["consumes"],
        ),
        diff_section(
            "workflowStructure",
            from_snapshot,
            to_snapshot,
            &["workflowStructure"],
        ),
        diff_section(
            "safetyLimits",
            from_snapshot,
            to_snapshot,
            &["safetyLimits", "capabilityFlags"],
        ),
    ]
}

fn diff_section(
    section: &str,
    from_snapshot: &JsonValue,
    to_snapshot: &JsonValue,
    fields: &[&str],
) -> JsonValue {
    let before = snapshot_fields(from_snapshot, fields);
    let after = snapshot_fields(to_snapshot, fields);
    json!({
        "section": section,
        "fields": fields,
        "changed": before != after,
        "before": before,
        "after": after,
    })
}

fn snapshot_fields(snapshot: &JsonValue, fields: &[&str]) -> JsonValue {
    let mut values = JsonMap::new();
    for field in fields {
        values.insert(
            (*field).to_string(),
            snapshot.get(*field).cloned().unwrap_or(JsonValue::Null),
        );
    }
    JsonValue::Object(values)
}

fn parse_json_column(label: &str, value: &str) -> Result<JsonValue, CliError> {
    serde_json::from_str(value).map_err(|error| {
        CliError::user_fixable(
            "xero_cli_agent_definition_json_invalid",
            format!("Could not decode {label}: {error}"),
        )
    })
}

fn parse_version(value: &str) -> Result<i64, CliError> {
    value
        .parse::<i64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| CliError::usage("`--version` must be a positive integer."))
}

fn reject_agent_definition_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if let Some(extra) = args.get(1) {
        return Err(CliError::usage(format!("Unexpected argument `{extra}`.")));
    }
    Ok(())
}

fn sqlite_agent_definition_error(operation: &str, error: rusqlite::Error) -> CliError {
    CliError::system_fault(
        "xero_cli_agent_definition_sql_failed",
        format!("Agent definition {operation} failed: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn list_reads_seeded_agent_definition_from_project_state() {
        let state_dir = temp_dir("agent-definition-state");
        let repo = temp_dir("agent-definition-repo");
        crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project",
            "import",
            "--path",
            repo.to_str().expect("repo"),
        ])
        .expect("project import");
        let output = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent-definition",
            "list",
        ])
        .expect("agent definition list");
        assert_eq!(output.json["kind"], json!("agentDefinitionList"));
        assert!(output.json["definitions"]
            .as_array()
            .expect("definitions")
            .iter()
            .any(|definition| definition["definitionId"] == json!("engineer")));
    }

    #[test]
    fn show_reads_current_agent_definition_snapshot_from_project_state() {
        let state_dir = temp_dir("agent-definition-show-state");
        let repo = temp_dir("agent-definition-show-repo");
        crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project",
            "import",
            "--path",
            repo.to_str().expect("repo"),
        ])
        .expect("project import");
        let output = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent-definition",
            "show",
            "engineer",
        ])
        .expect("agent definition show");
        assert_eq!(output.json["kind"], json!("agentDefinitionShow"));
        assert_eq!(
            output.json["definition"]["snapshot"]["id"],
            json!("engineer")
        );
    }

    #[test]
    fn versions_lists_saved_definition_versions_from_project_state() {
        let state_dir = temp_dir("agent-definition-versions-state");
        let repo = temp_dir("agent-definition-versions-repo");
        crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project",
            "import",
            "--path",
            repo.to_str().expect("repo"),
        ])
        .expect("project import");
        let output = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent-definition",
            "versions",
            "engineer",
        ])
        .expect("agent definition versions");
        assert_eq!(output.json["kind"], json!("agentDefinitionVersions"));
        assert!(output.json["versions"]
            .as_array()
            .expect("versions")
            .iter()
            .any(|version| version["definitionId"] == json!("engineer")));
    }

    #[test]
    fn diff_reports_changed_agent_definition_sections() {
        let state_dir = temp_dir("agent-definition-diff-state");
        let repo = temp_dir("agent-definition-diff-repo");
        crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project",
            "import",
            "--path",
            repo.to_str().expect("repo"),
        ])
        .expect("project import");
        seed_custom_definition(&state_dir, "project_researcher");

        let output = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent-definition",
            "diff",
            "project_researcher",
            "--from",
            "1",
            "--to",
            "2",
        ])
        .expect("agent definition diff");
        assert_eq!(output.json["kind"], json!("agentDefinitionDiff"));
        assert!(output.json["diff"]["changedSections"]
            .as_array()
            .expect("changed sections")
            .contains(&json!("workflowStructure")));
    }

    #[test]
    fn archive_marks_custom_definition_archived_without_touching_built_ins() {
        let state_dir = temp_dir("agent-definition-archive-state");
        let repo = temp_dir("agent-definition-archive-repo");
        crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project",
            "import",
            "--path",
            repo.to_str().expect("repo"),
        ])
        .expect("project import");
        seed_custom_definition(&state_dir, "project_researcher");

        let built_in_error = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent-definition",
            "archive",
            "engineer",
        ])
        .expect_err("built-in archive rejected");
        assert_eq!(
            built_in_error.code,
            "xero_cli_agent_definition_builtin_immutable"
        );

        let archived = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent-definition",
            "archive",
            "project_researcher",
        ])
        .expect("archive custom definition");
        assert_eq!(archived.json["kind"], json!("agentDefinitionArchive"));
        assert_eq!(
            archived.json["definition"]["lifecycleState"],
            json!("archived")
        );

        let visible = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "agent-definition",
            "list",
        ])
        .expect("list active definitions");
        assert!(!visible.json["definitions"]
            .as_array()
            .expect("definitions")
            .iter()
            .any(|definition| definition["definitionId"] == json!("project_researcher")));
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("{prefix}-{nonce}"));
        fs::create_dir_all(&path).expect("create temp dir");
        Command::new("git")
            .arg("init")
            .current_dir(&path)
            .output()
            .expect("git init");
        path
    }

    fn seed_custom_definition(state_dir: &Path, definition_id: &str) {
        let project_database = state_dir
            .join("projects")
            .join(project_id_for_repo_seed(state_dir))
            .join("state.db");
        let connection = rusqlite::Connection::open(project_database).expect("open project db");
        connection
            .execute(
                r#"
                INSERT INTO agent_definitions (
                    definition_id, current_version, display_name, short_label, scope,
                    lifecycle_state, base_capability_profile, updated_at
                )
                VALUES (?1, 2, 'Project Researcher', 'Research', 'project_custom',
                        'active', 'repository_recon', '2026-05-15T00:00:00Z')
                "#,
                params![definition_id],
            )
            .expect("insert custom definition");
        for (version, task_purpose, workflow_id) in [
            (1, "Research project facts.", "stage_research"),
            (2, "Research project facts and write notes.", "stage_write"),
        ] {
            connection
                .execute(
                    r#"
                    INSERT INTO agent_definition_versions (
                        definition_id, version, snapshot_json, validation_report_json, created_at
                    )
                    VALUES (?1, ?2, ?3, '{"status":"valid","source":"test"}', ?4)
                    "#,
                    params![
                        definition_id,
                        version,
                        json!({
                            "id": definition_id,
                            "version": version,
                            "displayName": "Project Researcher",
                            "taskPurpose": task_purpose,
                            "workflowStructure": {
                                "startPhaseId": workflow_id,
                                "phases": [{
                                    "id": workflow_id,
                                    "name": "Research"
                                }]
                            }
                        })
                        .to_string(),
                        format!("2026-05-15T00:00:0{version}Z")
                    ],
                )
                .expect("insert custom definition version");
        }
    }

    fn project_id_for_repo_seed(state_dir: &Path) -> String {
        let registry =
            rusqlite::Connection::open(state_dir.join("xero.db")).expect("open registry");
        registry
            .query_row(
                "SELECT id FROM projects ORDER BY updated_at DESC LIMIT 1",
                [],
                |row| row.get::<_, String>(0),
            )
            .expect("seeded project id")
    }
}
