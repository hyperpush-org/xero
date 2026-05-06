use std::path::Path;

use rusqlite::{params, OptionalExtension, Row};
use serde_json::Value as JsonValue;

use crate::{
    commands::{CommandError, RuntimeAgentIdDto, RuntimeRunApprovalModeDto},
    db::database_path_for_repo,
};

use super::{open_runtime_database, validate_non_empty_text};

pub const BUILTIN_AGENT_DEFINITION_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDefinitionRecord {
    pub definition_id: String,
    pub current_version: u32,
    pub display_name: String,
    pub short_label: String,
    pub description: String,
    pub scope: String,
    pub lifecycle_state: String,
    pub base_capability_profile: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDefinitionVersionRecord {
    pub definition_id: String,
    pub version: u32,
    pub snapshot: JsonValue,
    pub validation_report: Option<JsonValue>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentDefinitionRunSelection {
    pub definition_id: String,
    pub version: u32,
    pub display_name: String,
    pub base_capability_profile: String,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub default_approval_mode: RuntimeRunApprovalModeDto,
    pub allowed_approval_modes: Vec<RuntimeRunApprovalModeDto>,
    pub snapshot: JsonValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentDefinitionRecord {
    pub definition_id: String,
    pub version: u32,
    pub display_name: String,
    pub short_label: String,
    pub description: String,
    pub scope: String,
    pub lifecycle_state: String,
    pub base_capability_profile: String,
    pub snapshot: JsonValue,
    pub validation_report: Option<JsonValue>,
    pub created_at: String,
    pub updated_at: String,
}

pub fn default_agent_definition_id_for_runtime_agent(agent_id: RuntimeAgentIdDto) -> String {
    agent_id.as_str().to_string()
}

pub fn default_agent_definition_version_for_runtime_agent(_agent_id: RuntimeAgentIdDto) -> u32 {
    BUILTIN_AGENT_DEFINITION_VERSION
}

pub fn insert_agent_definition(
    repo_root: &Path,
    record: &NewAgentDefinitionRecord,
) -> Result<AgentDefinitionRecord, CommandError> {
    validate_new_agent_definition(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::retryable(
            "agent_definition_transaction_failed",
            format!(
                "Xero could not start the agent-definition transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;
    let snapshot_json = serde_json::to_string(&record.snapshot).map_err(|error| {
        CommandError::system_fault(
            "agent_definition_snapshot_serialize_failed",
            format!("Xero could not serialize the agent-definition snapshot: {error}"),
        )
    })?;
    let validation_report_json = record
        .validation_report
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_definition_validation_report_serialize_failed",
                format!("Xero could not serialize the agent-definition validation report: {error}"),
            )
        })?;

    transaction
        .execute(
            r#"
            INSERT INTO agent_definitions (
                definition_id,
                current_version,
                display_name,
                short_label,
                description,
                scope,
                lifecycle_state,
                base_capability_profile,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(definition_id) DO UPDATE SET
                current_version = excluded.current_version,
                display_name = excluded.display_name,
                short_label = excluded.short_label,
                description = excluded.description,
                scope = excluded.scope,
                lifecycle_state = excluded.lifecycle_state,
                base_capability_profile = excluded.base_capability_profile,
                updated_at = excluded.updated_at
            "#,
            params![
                record.definition_id,
                record.version,
                record.display_name,
                record.short_label,
                record.description,
                record.scope,
                record.lifecycle_state,
                record.base_capability_profile,
                record.created_at,
                record.updated_at,
            ],
        )
        .map_err(|error| {
            map_agent_definition_write_error("agent_definition_upsert_failed", error)
        })?;

    transaction
        .execute(
            r#"
            INSERT INTO agent_definition_versions (
                definition_id,
                version,
                snapshot_json,
                validation_report_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5)
            "#,
            params![
                record.definition_id,
                record.version,
                snapshot_json,
                validation_report_json,
                record.created_at,
            ],
        )
        .map_err(|error| {
            map_agent_definition_write_error("agent_definition_version_insert_failed", error)
        })?;

    transaction.commit().map_err(|error| {
        CommandError::retryable(
            "agent_definition_commit_failed",
            format!(
                "Xero could not commit the agent-definition transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    load_agent_definition(repo_root, &record.definition_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "agent_definition_insert_missing",
            "Xero saved an agent definition but could not load it back.",
        )
    })
}

pub fn load_agent_definition(
    repo_root: &Path,
    definition_id: &str,
) -> Result<Option<AgentDefinitionRecord>, CommandError> {
    validate_non_empty_text(
        definition_id,
        "definitionId",
        "agent_definition_request_invalid",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .query_row(
            r#"
            SELECT
                definition_id,
                current_version,
                display_name,
                short_label,
                description,
                scope,
                lifecycle_state,
                base_capability_profile,
                created_at,
                updated_at
            FROM agent_definitions
            WHERE definition_id = ?1
            "#,
            params![definition_id],
            read_agent_definition_row,
        )
        .optional()
        .map_err(|error| map_agent_definition_read_error("agent_definition_read_failed", error))
        .and_then(|row| row.transpose())
}

pub fn load_agent_definition_version(
    repo_root: &Path,
    definition_id: &str,
    version: u32,
) -> Result<Option<AgentDefinitionVersionRecord>, CommandError> {
    validate_non_empty_text(
        definition_id,
        "definitionId",
        "agent_definition_request_invalid",
    )?;
    if version == 0 {
        return Err(CommandError::invalid_request("version"));
    }
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .query_row(
            r#"
            SELECT
                definition_id,
                version,
                snapshot_json,
                validation_report_json,
                created_at
            FROM agent_definition_versions
            WHERE definition_id = ?1
              AND version = ?2
            "#,
            params![definition_id, version],
            read_agent_definition_version_row,
        )
        .optional()
        .map_err(|error| {
            map_agent_definition_read_error("agent_definition_version_read_failed", error)
        })
        .and_then(|row| row.transpose())
}

pub fn list_agent_definitions(
    repo_root: &Path,
    include_archived: bool,
) -> Result<Vec<AgentDefinitionRecord>, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let sql = if include_archived {
        r#"
        SELECT
            definition_id,
            current_version,
            display_name,
            short_label,
            description,
            scope,
            lifecycle_state,
            base_capability_profile,
            created_at,
            updated_at
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
        SELECT
            definition_id,
            current_version,
            display_name,
            short_label,
            description,
            scope,
            lifecycle_state,
            base_capability_profile,
            created_at,
            updated_at
        FROM agent_definitions
        WHERE lifecycle_state != 'archived'
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
    let mut statement = connection.prepare(sql).map_err(|error| {
        map_agent_definition_read_error("agent_definition_list_prepare_failed", error)
    })?;
    let rows = statement
        .query_map([], read_agent_definition_row)
        .map_err(|error| map_agent_definition_read_error("agent_definition_list_failed", error))?;
    let mut definitions = Vec::new();
    for row in rows {
        let definition = row.map_err(|error| {
            map_agent_definition_read_error("agent_definition_list_row_failed", error)
        })??;
        definitions.push(definition);
    }
    Ok(definitions)
}

pub fn archive_agent_definition(
    repo_root: &Path,
    definition_id: &str,
    updated_at: &str,
) -> Result<AgentDefinitionRecord, CommandError> {
    validate_non_empty_text(
        definition_id,
        "definitionId",
        "agent_definition_request_invalid",
    )?;
    validate_non_empty_text(updated_at, "updatedAt", "agent_definition_request_invalid")?;
    let definition = load_agent_definition(repo_root, definition_id)?.ok_or_else(|| {
        CommandError::user_fixable(
            "agent_definition_not_found",
            format!("Xero could not find agent definition `{definition_id}`."),
        )
    })?;
    if definition.scope == "built_in" {
        return Err(CommandError::user_fixable(
            "agent_definition_builtin_immutable",
            format!(
                "Xero cannot archive built-in agent definition `{}`.",
                definition.definition_id
            ),
        ));
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .execute(
            r#"
            UPDATE agent_definitions
            SET lifecycle_state = 'archived',
                updated_at = ?2
            WHERE definition_id = ?1
            "#,
            params![definition_id, updated_at],
        )
        .map_err(|error| {
            map_agent_definition_write_error("agent_definition_archive_failed", error)
        })?;

    load_agent_definition(repo_root, definition_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "agent_definition_archive_missing",
            "Xero archived an agent definition but could not load it back.",
        )
    })
}

pub fn resolve_agent_definition_for_run(
    repo_root: &Path,
    requested_definition_id: Option<&str>,
    fallback_runtime_agent_id: RuntimeAgentIdDto,
) -> Result<AgentDefinitionRunSelection, CommandError> {
    let definition_id = requested_definition_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            default_agent_definition_id_for_runtime_agent(fallback_runtime_agent_id)
        });
    let definition = load_agent_definition(repo_root, &definition_id)?.ok_or_else(|| {
        CommandError::user_fixable(
            "agent_definition_not_found",
            format!("Xero could not find active agent definition `{definition_id}`."),
        )
    })?;
    if definition.lifecycle_state != "active" {
        return Err(CommandError::user_fixable(
            "agent_definition_inactive",
            format!(
                "Xero cannot start a run from `{}` because the definition is `{}`.",
                definition.definition_id, definition.lifecycle_state
            ),
        ));
    }
    let version = load_agent_definition_version(
        repo_root,
        &definition.definition_id,
        definition.current_version,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "agent_definition_version_missing",
            format!(
                "Xero resolved `{}` but could not load version {}.",
                definition.definition_id, definition.current_version
            ),
        )
    })?;
    let runtime_agent_id =
        runtime_agent_id_for_base_capability_profile(&definition.base_capability_profile);
    let default_approval_mode =
        definition_default_approval_mode(&version.snapshot, runtime_agent_id);
    let allowed_approval_modes =
        definition_allowed_approval_modes(&version.snapshot, runtime_agent_id);
    Ok(AgentDefinitionRunSelection {
        runtime_agent_id,
        definition_id: definition.definition_id,
        version: definition.current_version,
        display_name: definition.display_name,
        base_capability_profile: definition.base_capability_profile,
        default_approval_mode,
        allowed_approval_modes,
        snapshot: version.snapshot,
    })
}

fn read_agent_definition_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentDefinitionRecord, CommandError>> {
    Ok(Ok(AgentDefinitionRecord {
        definition_id: row.get(0)?,
        current_version: read_positive_u32(row, 1)?,
        display_name: row.get(2)?,
        short_label: row.get(3)?,
        description: row.get(4)?,
        scope: row.get(5)?,
        lifecycle_state: row.get(6)?,
        base_capability_profile: row.get(7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    }))
}

fn read_agent_definition_version_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentDefinitionVersionRecord, CommandError>> {
    let snapshot_json: String = row.get(2)?;
    let snapshot = serde_json::from_str(&snapshot_json).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(error))
    })?;
    let validation_report_json: Option<String> = row.get(3)?;
    let validation_report = validation_report_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
    Ok(Ok(AgentDefinitionVersionRecord {
        definition_id: row.get(0)?,
        version: read_positive_u32(row, 1)?,
        snapshot,
        validation_report,
        created_at: row.get(4)?,
    }))
}

fn read_positive_u32(row: &Row<'_>, index: usize) -> rusqlite::Result<u32> {
    let value: i64 = row.get(index)?;
    u32::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn validate_new_agent_definition(record: &NewAgentDefinitionRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.definition_id,
        "definitionId",
        "agent_definition_request_invalid",
    )?;
    validate_non_empty_text(
        &record.display_name,
        "displayName",
        "agent_definition_request_invalid",
    )?;
    validate_non_empty_text(
        &record.short_label,
        "shortLabel",
        "agent_definition_request_invalid",
    )?;
    validate_known_agent_definition_value(
        "scope",
        &record.scope,
        &["built_in", "global_custom", "project_custom"],
    )?;
    validate_known_agent_definition_value(
        "lifecycleState",
        &record.lifecycle_state,
        &["draft", "active", "archived"],
    )?;
    validate_known_agent_definition_value(
        "baseCapabilityProfile",
        &record.base_capability_profile,
        &[
            "observe_only",
            "engineering",
            "debugging",
            "agent_builder",
            "harness_test",
        ],
    )?;
    if record.version == 0 {
        return Err(CommandError::invalid_request("version"));
    }
    if record.snapshot.is_null() {
        return Err(CommandError::invalid_request("snapshot"));
    }
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_definition_request_invalid",
    )?;
    validate_non_empty_text(
        &record.updated_at,
        "updatedAt",
        "agent_definition_request_invalid",
    )?;
    Ok(())
}

fn validate_known_agent_definition_value(
    field: &'static str,
    value: &str,
    allowed: &[&str],
) -> Result<(), CommandError> {
    if allowed.contains(&value) {
        return Ok(());
    }
    Err(CommandError::invalid_request(field))
}

pub fn runtime_agent_id_for_base_capability_profile(profile: &str) -> RuntimeAgentIdDto {
    match profile {
        "engineering" => RuntimeAgentIdDto::Engineer,
        "debugging" => RuntimeAgentIdDto::Debug,
        "agent_builder" => RuntimeAgentIdDto::AgentCreate,
        "harness_test" => RuntimeAgentIdDto::Test,
        _ => RuntimeAgentIdDto::Ask,
    }
}

fn definition_default_approval_mode(
    snapshot: &JsonValue,
    runtime_agent_id: RuntimeAgentIdDto,
) -> RuntimeRunApprovalModeDto {
    snapshot
        .get("defaultApprovalMode")
        .and_then(JsonValue::as_str)
        .and_then(parse_runtime_approval_mode)
        .filter(|mode| runtime_agent_allows_definition_mode(runtime_agent_id, mode))
        .unwrap_or_else(|| crate::commands::default_runtime_agent_approval_mode(&runtime_agent_id))
}

fn definition_allowed_approval_modes(
    snapshot: &JsonValue,
    runtime_agent_id: RuntimeAgentIdDto,
) -> Vec<RuntimeRunApprovalModeDto> {
    let mut modes = snapshot
        .get("allowedApprovalModes")
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .filter_map(parse_runtime_approval_mode)
                .filter(|mode| runtime_agent_allows_definition_mode(runtime_agent_id, mode))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if modes.is_empty() {
        modes = crate::commands::runtime_agent_allowed_approval_modes(&runtime_agent_id);
    }
    if !modes.contains(&RuntimeRunApprovalModeDto::Suggest) {
        modes.insert(0, RuntimeRunApprovalModeDto::Suggest);
    }
    modes.sort_by_key(|mode| match mode {
        RuntimeRunApprovalModeDto::Suggest => 0,
        RuntimeRunApprovalModeDto::AutoEdit => 1,
        RuntimeRunApprovalModeDto::Yolo => 2,
    });
    modes.dedup();
    modes
}

fn runtime_agent_allows_definition_mode(
    runtime_agent_id: RuntimeAgentIdDto,
    mode: &RuntimeRunApprovalModeDto,
) -> bool {
    crate::commands::runtime_agent_allows_approval_mode(&runtime_agent_id, mode)
}

fn parse_runtime_approval_mode(value: &str) -> Option<RuntimeRunApprovalModeDto> {
    match value.trim() {
        "suggest" => Some(RuntimeRunApprovalModeDto::Suggest),
        "auto_edit" => Some(RuntimeRunApprovalModeDto::AutoEdit),
        "yolo" => Some(RuntimeRunApprovalModeDto::Yolo),
        _ => None,
    }
}

fn map_agent_definition_write_error(code: &'static str, error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero could not write the agent-definition registry: {error}"),
    )
}

fn map_agent_definition_read_error(code: &'static str, error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero could not read the agent-definition registry: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{fs, path::PathBuf};

    use rusqlite::{params, Connection};
    use serde_json::json;

    use crate::db::{
        configure_connection, database_path_for_repo, migrations::migrations, project_store,
    };

    fn create_project_database(repo_root: &Path, project_id: &str) -> PathBuf {
        let database_path = repo_root
            .parent()
            .expect("repo parent")
            .join("app-data")
            .join("projects")
            .join(project_id)
            .join("state.db");
        fs::create_dir_all(database_path.parent().expect("database parent"))
            .expect("create database dir");
        let mut connection = Connection::open(&database_path).expect("open project database");
        configure_connection(&connection).expect("configure project database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate project database");
        connection
            .execute(
                "INSERT INTO projects (id, name, description, milestone) VALUES (?1, 'Project', '', '')",
                params![project_id],
            )
            .expect("insert project");
        connection
            .execute(
                r#"
                INSERT INTO repositories (id, project_id, root_path, display_name, branch, head_sha, is_git_repo)
                VALUES ('repo-1', ?1, ?2, 'Project', 'main', 'abc123', 1)
                "#,
                params![project_id, repo_root.to_string_lossy().as_ref()],
            )
            .expect("insert repository");
        connection
            .execute(
                r#"
                INSERT INTO agent_sessions (
                    project_id,
                    agent_session_id,
                    title,
                    status,
                    selected,
                    created_at,
                    updated_at
                )
                VALUES (?1, 'agent-session-main', 'Main', 'active', 1, ?2, ?2)
                "#,
                params![project_id, "2026-05-01T12:00:00Z"],
            )
            .expect("insert agent session");
        crate::db::register_project_database_path(repo_root, &database_path);
        database_path
    }

    fn custom_definition(version: u32, now: &str) -> NewAgentDefinitionRecord {
        NewAgentDefinitionRecord {
            definition_id: "project_researcher".into(),
            version,
            display_name: "Project Researcher".into(),
            short_label: "Research".into(),
            description: "Answer project questions using observe-only context.".into(),
            scope: "project_custom".into(),
            lifecycle_state: "active".into(),
            base_capability_profile: "observe_only".into(),
            snapshot: json!({
                "id": "project_researcher",
                "version": version,
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "observe_only",
                "label": "Project Researcher",
                "shortLabel": "Research"
            }),
            validation_report: Some(json!({
                "status": "valid",
                "source": "phase2_test"
            })),
            created_at: now.into(),
            updated_at: now.into(),
        }
    }

    #[test]
    fn custom_definition_selection_pins_run_version_across_reload() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        let project_id = "project-custom-definition";
        let database_path = create_project_database(&repo_root, project_id);

        insert_agent_definition(&repo_root, &custom_definition(1, "2026-05-01T12:01:00Z"))
            .expect("insert custom definition version 1");

        let inserted_run = project_store::insert_agent_run(
            &repo_root,
            &project_store::NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: Some("project_researcher".into()),
                agent_definition_version: None,
                project_id: project_id.into(),
                agent_session_id: "agent-session-main".into(),
                run_id: "run-custom-definition-1".into(),
                provider_id: "fake_provider".into(),
                model_id: "fake-model".into(),
                prompt: "Summarize the project constraints.".into(),
                system_prompt: "xero-owned-agent-v1".into(),
                now: "2026-05-01T12:02:00Z".into(),
            },
        )
        .expect("insert run with custom definition");

        assert_eq!(inserted_run.run.runtime_agent_id, RuntimeAgentIdDto::Ask);
        assert_eq!(inserted_run.run.agent_definition_id, "project_researcher");
        assert_eq!(inserted_run.run.agent_definition_version, 1);

        insert_agent_definition(&repo_root, &custom_definition(2, "2026-05-01T12:03:00Z"))
            .expect("insert custom definition version 2");

        let current_definition = load_agent_definition(&repo_root, "project_researcher")
            .expect("load current definition")
            .expect("current definition exists");
        assert_eq!(current_definition.current_version, 2);

        let reloaded_run =
            project_store::load_agent_run(&repo_root, project_id, "run-custom-definition-1")
                .expect("reload pinned run after definition update");
        assert_eq!(database_path_for_repo(&repo_root), database_path);
        assert_eq!(reloaded_run.run.agent_definition_id, "project_researcher");
        assert_eq!(reloaded_run.run.agent_definition_version, 1);

        let version_one = load_agent_definition_version(&repo_root, "project_researcher", 1)
            .expect("load version 1 snapshot")
            .expect("version 1 snapshot exists");
        assert_eq!(version_one.snapshot["version"], json!(1));

        let connection = Connection::open(database_path).expect("reopen project database");
        let immutable = connection
            .execute(
                r#"
                UPDATE agent_definition_versions
                SET snapshot_json = '{"id":"project_researcher","version":99}'
                WHERE definition_id = 'project_researcher'
                  AND version = 1
                "#,
                [],
            )
            .expect_err("definition versions remain immutable");
        assert!(immutable
            .to_string()
            .contains("agent definition versions are immutable"));
    }

    #[test]
    fn built_in_test_agent_definition_resolves_to_harness_runtime_agent() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        create_project_database(&repo_root, "project-test-definition");

        let selection = resolve_agent_definition_for_run(&repo_root, None, RuntimeAgentIdDto::Test)
            .expect("resolve built-in test definition");

        assert_eq!(selection.definition_id, "test");
        assert_eq!(selection.version, BUILTIN_AGENT_DEFINITION_VERSION);
        assert_eq!(selection.display_name, "Test");
        assert_eq!(selection.base_capability_profile, "harness_test");
        assert_eq!(selection.runtime_agent_id, RuntimeAgentIdDto::Test);
        assert_eq!(
            selection.default_approval_mode,
            RuntimeRunApprovalModeDto::Suggest
        );
        assert_eq!(
            selection.allowed_approval_modes,
            vec![RuntimeRunApprovalModeDto::Suggest]
        );
        assert_eq!(selection.snapshot["scope"], json!("built_in"));
        assert_eq!(
            selection.snapshot["baseCapabilityProfile"],
            json!("harness_test")
        );
        assert_eq!(selection.snapshot["promptPolicy"], json!("harness_test"));
        assert_eq!(selection.snapshot["toolPolicy"], json!("harness_test"));
        assert_eq!(
            selection.snapshot["outputContract"],
            json!("harness_test_report")
        );
    }
}
