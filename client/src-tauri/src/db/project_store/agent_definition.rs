use std::path::Path;

use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use crate::{
    commands::{CommandError, RuntimeAgentIdDto, RuntimeRunApprovalModeDto},
    db::database_path_for_repo,
};

use super::{
    agent_runtime_audit_id, capability_permission_explanation, open_runtime_database,
    record_agent_runtime_audit_event, validate_non_empty_text, NewAgentRuntimeAuditEventRecord,
};

pub const BUILTIN_AGENT_DEFINITION_VERSION: u32 = 1;
const AGENT_ACTIVATION_PREFLIGHT_SCHEMA: &str = "xero.custom_agent_activation_preflight.v1";
const AGENT_ACTIVATION_PREFLIGHT_SCHEMA_VERSION: u64 = 1;
const AGENT_DEFINITION_SCHEMA: &str = "xero.agent_definition.v1";
const AGENT_DEFINITION_SCHEMA_VERSION: u64 = 2;

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

    if let Ok(project_id) = project_id_for_agent_definition_audit(repo_root) {
        let _ = record_agent_runtime_audit_event(
            repo_root,
            &NewAgentRuntimeAuditEventRecord {
                audit_id: agent_runtime_audit_id(
                    &project_id,
                    "agent_definition_saved",
                    "custom_agent",
                    &record.definition_id,
                    &record.updated_at,
                ),
                project_id,
                actor_kind: "user".into(),
                actor_id: None,
                action_kind: "agent_definition_saved".into(),
                subject_kind: "custom_agent".into(),
                subject_id: record.definition_id.clone(),
                run_id: None,
                agent_definition_id: Some(record.definition_id.clone()),
                agent_definition_version: Some(record.version),
                risk_class: capability_permission_explanation(
                    "custom_agent",
                    &record.definition_id,
                )
                .get("riskClass")
                .and_then(JsonValue::as_str)
                .map(ToOwned::to_owned),
                approval_action_id: None,
                payload: serde_json::json!({
                    "definitionId": &record.definition_id,
                    "version": record.version,
                    "scope": &record.scope,
                    "lifecycleState": &record.lifecycle_state,
                    "baseCapabilityProfile": &record.base_capability_profile,
                    "validationStatus": record.validation_report.as_ref().and_then(|report| report.get("status")).and_then(JsonValue::as_str),
                    "attachedSkills": attached_skill_audit_summary(Some(&record.snapshot)),
                }),
                created_at: record.updated_at.clone(),
            },
        );
    }

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

pub fn load_agent_definition_version_diff(
    repo_root: &Path,
    definition_id: &str,
    from_version: u32,
    to_version: u32,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(
        definition_id,
        "definitionId",
        "agent_definition_diff_request_invalid",
    )?;
    if from_version == 0 || to_version == 0 {
        return Err(CommandError::invalid_request("version"));
    }
    let from = load_agent_definition_version(repo_root, definition_id, from_version)?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_definition_diff_version_missing",
                format!(
                    "Xero could not find version {from_version} of agent definition `{definition_id}`."
                ),
            )
        })?;
    let to = load_agent_definition_version(repo_root, definition_id, to_version)?.ok_or_else(
        || {
            CommandError::user_fixable(
                "agent_definition_diff_version_missing",
                format!(
                    "Xero could not find version {to_version} of agent definition `{definition_id}`."
                ),
            )
        },
    )?;
    Ok(agent_definition_version_diff(&from, &to))
}

fn agent_definition_version_diff(
    from: &AgentDefinitionVersionRecord,
    to: &AgentDefinitionVersionRecord,
) -> JsonValue {
    let sections = [
        diff_section(
            "identity",
            &from.snapshot,
            &to.snapshot,
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
            &from.snapshot,
            &to.snapshot,
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
            &from.snapshot,
            &to.snapshot,
            &["attachedSkills"],
        ),
        diff_section(
            "toolPolicy",
            &from.snapshot,
            &to.snapshot,
            &["toolPolicy", "tools"],
        ),
        diff_section(
            "memoryPolicy",
            &from.snapshot,
            &to.snapshot,
            &["memoryPolicy"],
        ),
        diff_section(
            "retrievalPolicy",
            &from.snapshot,
            &to.snapshot,
            &["retrievalDefaults"],
        ),
        diff_section(
            "handoffPolicy",
            &from.snapshot,
            &to.snapshot,
            &["handoffPolicy"],
        ),
        diff_section(
            "outputContract",
            &from.snapshot,
            &to.snapshot,
            &["outputContract", "output"],
        ),
        diff_section(
            "databaseAccess",
            &from.snapshot,
            &to.snapshot,
            &["dbTouchpoints"],
        ),
        diff_section(
            "consumedArtifacts",
            &from.snapshot,
            &to.snapshot,
            &["consumes"],
        ),
        diff_section(
            "workflowStructure",
            &from.snapshot,
            &to.snapshot,
            &["workflowStructure"],
        ),
        diff_section(
            "safetyLimits",
            &from.snapshot,
            &to.snapshot,
            &["safetyLimits", "capabilityFlags"],
        ),
    ];
    let changed_sections = sections
        .iter()
        .filter(|section| section["changed"].as_bool() == Some(true))
        .filter_map(|section| section["section"].as_str().map(str::to_owned))
        .collect::<Vec<_>>();

    json!({
        "schema": "xero.agent_definition_version_diff.v1",
        "definitionId": from.definition_id,
        "fromVersion": from.version,
        "toVersion": to.version,
        "fromCreatedAt": from.created_at,
        "toCreatedAt": to.created_at,
        "changed": !changed_sections.is_empty(),
        "changedSections": changed_sections,
        "sections": sections,
    })
}

pub fn load_agent_activation_preflight(
    repo_root: &Path,
    definition_id: &str,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(
        definition_id,
        "definitionId",
        "agent_definition_preflight_request_invalid",
    )?;
    let definition = load_agent_definition(repo_root, definition_id)?.ok_or_else(|| {
        CommandError::user_fixable(
            "agent_definition_not_found",
            format!("Xero could not find agent definition `{definition_id}`."),
        )
    })?;
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

    Ok(custom_agent_activation_preflight_report(
        &definition,
        &version,
    ))
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

    let archived_version_snapshot =
        load_agent_definition_version(repo_root, definition_id, definition.current_version)
            .ok()
            .flatten()
            .map(|version| version.snapshot);

    if let Ok(project_id) = project_id_for_agent_definition_audit(repo_root) {
        let _ = record_agent_runtime_audit_event(
            repo_root,
            &NewAgentRuntimeAuditEventRecord {
                audit_id: agent_runtime_audit_id(
                    &project_id,
                    "agent_definition_archived",
                    "custom_agent",
                    definition_id,
                    updated_at,
                ),
                project_id,
                actor_kind: "user".into(),
                actor_id: None,
                action_kind: "agent_definition_archived".into(),
                subject_kind: "custom_agent".into(),
                subject_id: definition_id.to_string(),
                run_id: None,
                agent_definition_id: Some(definition_id.to_string()),
                agent_definition_version: Some(definition.current_version),
                risk_class: capability_permission_explanation("custom_agent", definition_id)
                    .get("riskClass")
                    .and_then(JsonValue::as_str)
                    .map(ToOwned::to_owned),
                approval_action_id: None,
                payload: serde_json::json!({
                    "definitionId": definition_id,
                    "previousLifecycleState": &definition.lifecycle_state,
                    "lifecycleState": "archived",
                    "attachedSkills": attached_skill_audit_summary(archived_version_snapshot.as_ref()),
                }),
                created_at: updated_at.to_string(),
            },
        );
    }

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
    if definition.scope != "built_in"
        && !agent_definition_validation_report_allows_activation(version.validation_report.as_ref())
    {
        return Err(CommandError::user_fixable(
            "agent_definition_activation_preflight_failed",
            format!(
                "Xero cannot start a run from `{}` because version {} does not have a valid custom-agent validation report.",
                definition.definition_id, definition.current_version
            ),
        ));
    }
    if definition.scope != "built_in" {
        validate_custom_agent_activation_consistency(&definition, &version)?;
        validate_custom_agent_activation_runtime_contract(&definition, &version)?;
    }
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

fn agent_definition_validation_report_allows_activation(report: Option<&JsonValue>) -> bool {
    report
        .and_then(|report| report.get("status"))
        .and_then(JsonValue::as_str)
        == Some("valid")
}

fn validate_custom_agent_activation_consistency(
    definition: &AgentDefinitionRecord,
    version: &AgentDefinitionVersionRecord,
) -> Result<(), CommandError> {
    activation_snapshot_string_matches(
        &version.snapshot,
        "id",
        &definition.definition_id,
        &definition.definition_id,
        version.version,
    )?;
    activation_snapshot_string_matches(
        &version.snapshot,
        "scope",
        &definition.scope,
        &definition.definition_id,
        version.version,
    )?;
    activation_snapshot_string_matches(
        &version.snapshot,
        "lifecycleState",
        &definition.lifecycle_state,
        &definition.definition_id,
        version.version,
    )?;
    activation_snapshot_string_matches(
        &version.snapshot,
        "baseCapabilityProfile",
        &definition.base_capability_profile,
        &definition.definition_id,
        version.version,
    )?;
    if let Some(snapshot_version) = version.snapshot.get("version").and_then(JsonValue::as_u64) {
        if snapshot_version != u64::from(version.version) {
            return Err(activation_preflight_error(
                &definition.definition_id,
                version.version,
                "version",
                &version.version.to_string(),
                &snapshot_version.to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_custom_agent_activation_runtime_contract(
    definition: &AgentDefinitionRecord,
    version: &AgentDefinitionVersionRecord,
) -> Result<(), CommandError> {
    let preflight = custom_agent_activation_preflight_report(definition, version);
    if preflight.get("status").and_then(JsonValue::as_str) == Some("passed") {
        return Ok(());
    }
    let (check_id, summary) = activation_preflight_first_blocker(&preflight).unwrap_or_else(|| {
        (
            "unknown".to_string(),
            "activation preflight reported a blocking failure".to_string(),
        )
    });
    Err(CommandError::user_fixable(
        "agent_definition_activation_preflight_failed",
        format!(
            "Xero cannot start a run from `{}` version {} because activation preflight failed `{check_id}`: {summary}",
            definition.definition_id, version.version
        ),
    ))
}

fn custom_agent_activation_preflight_report(
    definition: &AgentDefinitionRecord,
    version: &AgentDefinitionVersionRecord,
) -> JsonValue {
    let snapshot = &version.snapshot;
    let checks = vec![
        activation_check_from_failures(
            "saved_validation_report",
            "Saved validation report",
            validation_report_activation_failures(version.validation_report.as_ref()),
            "The pinned custom-agent version has a valid saved validation report.",
            true,
            json!({
                "validationStatus": version.validation_report.as_ref().and_then(|report| report.get("status")).and_then(JsonValue::as_str)
            }),
        ),
        activation_check_from_failures(
            "snapshot_consistency",
            "Pinned snapshot consistency",
            activation_snapshot_consistency_failures(definition, version),
            "The pinned snapshot agrees with the active definition registry row.",
            true,
            json!({
                "definitionId": definition.definition_id,
                "currentVersion": definition.current_version,
                "snapshotVersion": snapshot.get("version").cloned().unwrap_or(JsonValue::Null)
            }),
        ),
        activation_check_from_failures(
            "schema_metadata",
            "Canonical schema metadata",
            activation_schema_metadata_failures(snapshot),
            "The pinned snapshot uses the supported canonical custom-agent schema.",
            true,
            json!({
                "schema": snapshot.get("schema").cloned().unwrap_or(JsonValue::Null),
                "schemaVersion": snapshot.get("schemaVersion").cloned().unwrap_or(JsonValue::Null)
            }),
        ),
        activation_check_from_failures(
            "effective_prompt",
            "Effective prompt inputs",
            activation_prompt_failures(snapshot),
            "The snapshot contains prompt intent plus workflow and final-response contracts.",
            true,
            json!({
                "workflowContractPresent": activation_non_empty_text(snapshot, "workflowContract").is_some(),
                "finalResponseContractPresent": activation_non_empty_text(snapshot, "finalResponseContract").is_some(),
                "promptIntentPresent": activation_prompt_intent_present(snapshot)
            }),
        ),
        activation_check_from_failures(
            "effective_tools",
            "Effective tool policy",
            activation_tool_policy_failures(snapshot, &definition.base_capability_profile),
            "The approval and tool policy can be narrowed under the base capability profile.",
            true,
            json!({
                "baseCapabilityProfile": definition.base_capability_profile,
                "defaultApprovalMode": snapshot.get("defaultApprovalMode").cloned().unwrap_or(JsonValue::Null),
                "allowedApprovalModes": snapshot.get("allowedApprovalModes").cloned().unwrap_or(JsonValue::Null),
                "toolPolicy": snapshot.get("toolPolicy").cloned().unwrap_or(JsonValue::Null)
            }),
        ),
        activation_check_from_failures(
            "storage_access",
            "Storage access contract",
            activation_db_touchpoint_failures(snapshot.get("dbTouchpoints")),
            "The saved database touchpoint contract is structured enough for runtime guidance and audit metadata.",
            true,
            json!({
                "dbTouchpoints": snapshot.get("dbTouchpoints").cloned().unwrap_or(JsonValue::Null)
            }),
        ),
        activation_check_from_failures(
            "context_policy",
            "Context, memory, retrieval, and handoff policy",
            activation_context_policy_failures(snapshot),
            "The context, memory, retrieval, and handoff policies are runtime-readable.",
            true,
            json!({
                "projectDataPolicy": snapshot.get("projectDataPolicy").cloned().unwrap_or(JsonValue::Null),
                "memoryCandidatePolicy": snapshot.get("memoryCandidatePolicy").cloned().unwrap_or(JsonValue::Null),
                "retrievalDefaults": snapshot.get("retrievalDefaults").cloned().unwrap_or(JsonValue::Null),
                "handoffPolicy": snapshot.get("handoffPolicy").cloned().unwrap_or(JsonValue::Null)
            }),
        ),
        activation_check_from_failures(
            "attached_skills",
            "Attached skills contract",
            activation_attached_skill_failures(snapshot.get("attachedSkills")),
            "The snapshot declares an attachedSkills array with unique ids and source ids.",
            true,
            json!({
                "attachedSkills": snapshot.get("attachedSkills").cloned().unwrap_or(JsonValue::Null)
            }),
        ),
        activation_check_from_failures(
            "output_contract",
            "Output contract",
            activation_output_failures(snapshot.get("output")),
            "The output contract has a supported contract kind and at least one section.",
            true,
            json!({
                "output": snapshot.get("output").cloned().unwrap_or(JsonValue::Null)
            }),
        ),
        activation_check_from_failures(
            "risky_capability_confirmations",
            "Risky capability confirmations",
            activation_risky_capability_failures(snapshot.get("toolPolicy")),
            "Risky effect classes have explicit matching confirmation flags.",
            true,
            json!({
                "toolPolicy": snapshot.get("toolPolicy").cloned().unwrap_or(JsonValue::Null)
            }),
        ),
    ];
    let failed = checks.iter().any(|check| {
        check.get("blocking").and_then(JsonValue::as_bool) == Some(true)
            && check.get("status").and_then(JsonValue::as_str) == Some("failed")
    });
    json!({
        "schema": AGENT_ACTIVATION_PREFLIGHT_SCHEMA,
        "schemaVersion": AGENT_ACTIVATION_PREFLIGHT_SCHEMA_VERSION,
        "status": if failed { "failed" } else { "passed" },
        "source": {
            "kind": "activation_run_selector",
            "uiDeferred": true,
            "uiDeferralReason": "The active implementation constraint forbids adding a new visible custom-agent preflight surface."
        },
        "definition": {
            "definitionId": definition.definition_id,
            "version": version.version,
            "displayName": definition.display_name,
            "scope": definition.scope,
            "lifecycleState": definition.lifecycle_state,
            "baseCapabilityProfile": definition.base_capability_profile
        },
        "checks": checks
    })
}

fn activation_preflight_first_blocker(preflight: &JsonValue) -> Option<(String, String)> {
    preflight
        .get("checks")
        .and_then(JsonValue::as_array)?
        .iter()
        .find(|check| {
            check.get("blocking").and_then(JsonValue::as_bool) == Some(true)
                && check.get("status").and_then(JsonValue::as_str) == Some("failed")
        })
        .map(|check| {
            let id = check
                .get("id")
                .and_then(JsonValue::as_str)
                .unwrap_or("unknown")
                .to_string();
            let summary = check
                .get("summary")
                .and_then(JsonValue::as_str)
                .unwrap_or("activation preflight failed")
                .to_string();
            (id, summary)
        })
}

fn activation_check_from_failures(
    id: &str,
    label: &str,
    failures: Vec<String>,
    passed_summary: &str,
    blocking: bool,
    details: JsonValue,
) -> JsonValue {
    let status = if failures.is_empty() {
        "passed"
    } else {
        "failed"
    };
    let summary = if failures.is_empty() {
        passed_summary.to_string()
    } else {
        failures.join("; ")
    };
    json!({
        "id": id,
        "label": label,
        "status": status,
        "blocking": blocking,
        "summary": summary,
        "failures": failures,
        "details": details
    })
}

fn validation_report_activation_failures(report: Option<&JsonValue>) -> Vec<String> {
    if agent_definition_validation_report_allows_activation(report) {
        Vec::new()
    } else {
        vec!["validation report status must be valid".to_string()]
    }
}

fn activation_snapshot_consistency_failures(
    definition: &AgentDefinitionRecord,
    version: &AgentDefinitionVersionRecord,
) -> Vec<String> {
    let mut failures = Vec::new();
    activation_required_string_equals(
        &version.snapshot,
        "id",
        &definition.definition_id,
        &mut failures,
    );
    activation_required_string_equals(&version.snapshot, "scope", &definition.scope, &mut failures);
    activation_required_string_equals(
        &version.snapshot,
        "lifecycleState",
        &definition.lifecycle_state,
        &mut failures,
    );
    activation_required_string_equals(
        &version.snapshot,
        "baseCapabilityProfile",
        &definition.base_capability_profile,
        &mut failures,
    );
    match version.snapshot.get("version").and_then(JsonValue::as_u64) {
        Some(snapshot_version) if snapshot_version == u64::from(version.version) => {}
        Some(snapshot_version) => failures.push(format!(
            "version is `{snapshot_version}` but registry selected `{}`",
            version.version
        )),
        None => failures.push("version must be present as a positive number".to_string()),
    }
    failures
}

fn activation_required_string_equals(
    snapshot: &JsonValue,
    field: &str,
    expected: &str,
    failures: &mut Vec<String>,
) {
    match snapshot.get(field).and_then(JsonValue::as_str) {
        Some(actual) if actual == expected => {}
        Some(actual) => failures.push(format!("{field} is `{actual}` but expected `{expected}`")),
        None => failures.push(format!("{field} must be present as `{expected}`")),
    }
}

fn activation_schema_metadata_failures(snapshot: &JsonValue) -> Vec<String> {
    let mut failures = Vec::new();
    match snapshot.get("schema").and_then(JsonValue::as_str) {
        Some(AGENT_DEFINITION_SCHEMA) => {}
        Some(schema) => failures.push(format!(
            "schema is `{schema}` but expected `{AGENT_DEFINITION_SCHEMA}`"
        )),
        None => failures.push(format!("schema must be `{AGENT_DEFINITION_SCHEMA}`")),
    }
    match snapshot.get("schemaVersion").and_then(JsonValue::as_u64) {
        Some(AGENT_DEFINITION_SCHEMA_VERSION) => {}
        Some(version) => failures.push(format!(
            "schemaVersion is `{version}` but expected `{AGENT_DEFINITION_SCHEMA_VERSION}`"
        )),
        None => failures.push(format!(
            "schemaVersion must be `{AGENT_DEFINITION_SCHEMA_VERSION}`"
        )),
    }
    failures
}

fn activation_prompt_failures(snapshot: &JsonValue) -> Vec<String> {
    let mut failures = Vec::new();
    if activation_non_empty_text(snapshot, "workflowContract").is_none() {
        failures.push("workflowContract must be non-empty".to_string());
    }
    if activation_non_empty_text(snapshot, "finalResponseContract").is_none() {
        failures.push("finalResponseContract must be non-empty".to_string());
    }
    if !activation_prompt_intent_present(snapshot) {
        failures.push(
            "at least one prompt body or prompt fragment must explain the agent intent".to_string(),
        );
    }
    failures
}

fn activation_prompt_intent_present(snapshot: &JsonValue) -> bool {
    snapshot
        .get("prompts")
        .and_then(JsonValue::as_array)
        .is_some_and(|prompts| {
            prompts.iter().any(|prompt| {
                prompt
                    .get("body")
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .is_some_and(|body| !body.is_empty())
            })
        })
        || snapshot
            .get("promptFragments")
            .is_some_and(activation_value_contains_non_empty_text)
}

fn activation_non_empty_text<'a>(snapshot: &'a JsonValue, field: &str) -> Option<&'a str> {
    snapshot
        .get(field)
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn activation_value_contains_non_empty_text(value: &JsonValue) -> bool {
    match value {
        JsonValue::String(value) => !value.trim().is_empty(),
        JsonValue::Array(values) => values.iter().any(activation_value_contains_non_empty_text),
        JsonValue::Object(object) => object
            .values()
            .any(activation_value_contains_non_empty_text),
        _ => false,
    }
}

fn activation_tool_policy_failures(snapshot: &JsonValue, base_profile: &str) -> Vec<String> {
    let mut failures = Vec::new();
    let default_mode = snapshot
        .get("defaultApprovalMode")
        .and_then(JsonValue::as_str)
        .unwrap_or_default();
    if !matches!(default_mode, "suggest" | "auto_edit" | "yolo") {
        failures.push("defaultApprovalMode must be suggest, auto_edit, or yolo".to_string());
    }
    let allowed_modes = activation_string_array(snapshot.get("allowedApprovalModes"));
    if allowed_modes.is_empty() {
        failures.push("allowedApprovalModes must include suggest".to_string());
    } else if !allowed_modes.iter().any(|mode| mode == "suggest") {
        failures.push("allowedApprovalModes must include suggest".to_string());
    }
    if matches!(
        base_profile,
        "observe_only" | "planning" | "repository_recon" | "agent_builder"
    ) && (default_mode != "suggest" || allowed_modes.iter().any(|mode| mode != "suggest"))
    {
        failures.push(
            "observe_only, planning, repository_recon, and agent_builder profiles can only use suggest approval mode"
                .to_string(),
        );
    }

    let Some(policy) = snapshot.get("toolPolicy") else {
        failures.push("toolPolicy is required".to_string());
        return failures;
    };
    if let Some(policy_name) = policy.as_str() {
        if !activation_string_tool_policy_allowed(base_profile, policy_name) {
            failures.push(format!(
                "string toolPolicy `{policy_name}` exceeds base profile `{base_profile}`"
            ));
        }
        return failures;
    }
    let Some(object) = policy.as_object() else {
        failures.push("toolPolicy must be a string or object".to_string());
        return failures;
    };
    for field in [
        "allowedTools",
        "deniedTools",
        "allowedToolPacks",
        "deniedToolPacks",
        "allowedToolGroups",
        "deniedToolGroups",
    ] {
        if let Some(value) = object.get(field) {
            activation_validate_string_array_field(
                value,
                format!("toolPolicy.{field}"),
                &mut failures,
            );
        }
    }
    if let Some(value) = object.get("allowedEffectClasses") {
        let effect_classes = activation_string_array(Some(value));
        if effect_classes.is_empty() && !value.as_array().is_some_and(Vec::is_empty) {
            failures
                .push("toolPolicy.allowedEffectClasses must be an array of strings".to_string());
        }
        for effect_class in effect_classes {
            if !activation_effect_allowed_by_profile(base_profile, &effect_class) {
                failures.push(format!(
                    "effect class `{effect_class}` exceeds base profile `{base_profile}`"
                ));
            }
        }
    }
    failures
}

fn activation_string_tool_policy_allowed(base_profile: &str, policy: &str) -> bool {
    match base_profile {
        "observe_only" => policy == "observe_only",
        "planning" => matches!(policy, "planning" | "observe_only"),
        "repository_recon" => matches!(policy, "repository_recon" | "observe_only"),
        "agent_builder" => matches!(policy, "agent_builder" | "observe_only"),
        "engineering" | "debugging" => matches!(policy, "observe_only" | "engineering"),
        _ => false,
    }
}

fn activation_effect_allowed_by_profile(base_profile: &str, effect_class: &str) -> bool {
    match base_profile {
        "observe_only" => effect_class == "observe",
        "planning" => matches!(effect_class, "observe" | "runtime_state"),
        "repository_recon" => matches!(
            effect_class,
            "observe" | "runtime_state" | "command" | "process_control"
        ),
        "agent_builder" => matches!(effect_class, "observe" | "runtime_state"),
        "engineering" | "debugging" => matches!(
            effect_class,
            "observe"
                | "runtime_state"
                | "write"
                | "destructive_write"
                | "command"
                | "process_control"
                | "browser_control"
                | "device_control"
                | "external_service"
                | "skill_runtime"
                | "agent_delegation"
        ),
        _ => false,
    }
}

fn activation_db_touchpoint_failures(value: Option<&JsonValue>) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(object) = value.and_then(JsonValue::as_object) else {
        failures.push("dbTouchpoints must be an object".to_string());
        return failures;
    };
    for field in ["reads", "writes", "encouraged"] {
        let Some(entries) = object.get(field).and_then(JsonValue::as_array) else {
            failures.push(format!("dbTouchpoints.{field} must be an array"));
            continue;
        };
        for (index, entry) in entries.iter().enumerate() {
            let path = format!("dbTouchpoints.{field}[{index}]");
            let Some(entry) = entry.as_object() else {
                failures.push(format!("{path} must be an object"));
                continue;
            };
            for required in ["table", "purpose"] {
                if entry
                    .get(required)
                    .and_then(JsonValue::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    failures.push(format!("{path}.{required} must be a non-empty string"));
                }
            }
            for required_array in ["triggers", "columns"] {
                if entry
                    .get(required_array)
                    .and_then(JsonValue::as_array)
                    .is_none()
                {
                    failures.push(format!("{path}.{required_array} must be an array"));
                }
            }
        }
    }
    failures
}

fn activation_context_policy_failures(snapshot: &JsonValue) -> Vec<String> {
    let mut failures = Vec::new();
    activation_policy_object(snapshot, "projectDataPolicy", &mut failures);
    activation_policy_object(snapshot, "memoryCandidatePolicy", &mut failures);
    let retrieval = activation_policy_object(snapshot, "retrievalDefaults", &mut failures);
    let handoff = activation_policy_object(snapshot, "handoffPolicy", &mut failures);
    if let Some(object) = retrieval {
        if object.get("enabled").and_then(JsonValue::as_bool).is_none() {
            failures.push("retrievalDefaults.enabled must be a boolean".to_string());
        }
        if let Some(limit) = object.get("limit") {
            if limit.as_u64().filter(|value| *value > 0).is_none() {
                failures.push("retrievalDefaults.limit must be a positive integer".to_string());
            }
        }
    }
    if let Some(object) = handoff {
        if object.get("enabled").and_then(JsonValue::as_bool).is_none() {
            failures.push("handoffPolicy.enabled must be a boolean".to_string());
        }
        if object
            .get("preserveDefinitionVersion")
            .and_then(JsonValue::as_bool)
            .is_none()
        {
            failures.push("handoffPolicy.preserveDefinitionVersion must be a boolean".to_string());
        }
    }
    failures
}

fn activation_attached_skill_failures(value: Option<&JsonValue>) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(skills) = value.and_then(JsonValue::as_array) else {
        failures.push("attachedSkills must be an array".to_string());
        return failures;
    };
    let mut ids = std::collections::BTreeSet::new();
    let mut source_ids = std::collections::BTreeSet::new();
    for (index, skill) in skills.iter().enumerate() {
        let path = format!("attachedSkills[{index}]");
        let Some(object) = skill.as_object() else {
            failures.push(format!("{path} must be an object"));
            continue;
        };
        for required in [
            "id",
            "sourceId",
            "skillId",
            "name",
            "sourceKind",
            "scope",
            "versionHash",
        ] {
            if object
                .get(required)
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .is_none()
            {
                failures.push(format!("{path}.{required} must be a non-empty string"));
            }
        }
        if object
            .get("includeSupportingAssets")
            .and_then(JsonValue::as_bool)
            .is_none()
        {
            failures.push(format!("{path}.includeSupportingAssets must be a boolean"));
        }
        if object.get("required").and_then(JsonValue::as_bool) != Some(true) {
            failures.push(format!("{path}.required must be true"));
        }
        if let Some(id) = object
            .get("id")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !ids.insert(id.to_string()) {
                failures.push(format!("{path}.id `{id}` is duplicated"));
            }
        }
        if let Some(source_id) = object
            .get("sourceId")
            .and_then(JsonValue::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            if !source_ids.insert(source_id.to_string()) {
                failures.push(format!("{path}.sourceId `{source_id}` is duplicated"));
            }
        }
    }
    failures
}

fn activation_policy_object<'a>(
    snapshot: &'a JsonValue,
    field: &str,
    failures: &mut Vec<String>,
) -> Option<&'a JsonMap<String, JsonValue>> {
    match snapshot.get(field) {
        Some(value) => match value.as_object() {
            Some(object) => Some(object),
            None => {
                failures.push(format!("{field} must be an object"));
                None
            }
        },
        None => {
            failures.push(format!("{field} must be present"));
            None
        }
    }
}

fn activation_output_failures(value: Option<&JsonValue>) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(object) = value.and_then(JsonValue::as_object) else {
        failures.push("output must be an object".to_string());
        return failures;
    };
    let supported_contracts = [
        "answer",
        "plan_pack",
        "crawl_report",
        "engineering_summary",
        "debug_summary",
        "agent_definition_draft",
        "harness_test_report",
    ];
    match object.get("contract").and_then(JsonValue::as_str) {
        Some(contract) if supported_contracts.contains(&contract.trim()) => {}
        Some(contract) => failures.push(format!("output.contract `{contract}` is not supported")),
        None => failures.push("output.contract is required".to_string()),
    }
    match object.get("sections").and_then(JsonValue::as_array) {
        Some(sections) if !sections.is_empty() => {}
        Some(_) => failures.push("output.sections must include at least one section".to_string()),
        None => failures.push("output.sections must be an array".to_string()),
    }
    failures
}

fn activation_risky_capability_failures(value: Option<&JsonValue>) -> Vec<String> {
    let mut failures = Vec::new();
    let Some(object) = value.and_then(JsonValue::as_object) else {
        return failures;
    };
    let effect_classes = activation_string_array(object.get("allowedEffectClasses"));
    for (effect_class, flag) in [
        ("external_service", "externalServiceAllowed"),
        ("browser_control", "browserControlAllowed"),
        ("skill_runtime", "skillRuntimeAllowed"),
        ("agent_delegation", "subagentAllowed"),
        ("command", "commandAllowed"),
        ("process_control", "commandAllowed"),
        ("destructive_write", "destructiveWriteAllowed"),
    ] {
        if effect_classes.iter().any(|class| class == effect_class)
            && object.get(flag).and_then(JsonValue::as_bool) != Some(true)
        {
            failures.push(format!(
                "toolPolicy.{flag} must be true when allowedEffectClasses includes `{effect_class}`"
            ));
        }
    }
    failures
}

fn activation_string_array(value: Option<&JsonValue>) -> Vec<String> {
    value
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn activation_validate_string_array_field(
    value: &JsonValue,
    field: String,
    failures: &mut Vec<String>,
) {
    let Some(items) = value.as_array() else {
        failures.push(format!("{field} must be an array of strings"));
        return;
    };
    if items.iter().any(|item| {
        item.as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .is_none()
    }) {
        failures.push(format!("{field} must contain only non-empty strings"));
    }
}

fn activation_snapshot_string_matches(
    snapshot: &JsonValue,
    field: &'static str,
    expected: &str,
    definition_id: &str,
    version: u32,
) -> Result<(), CommandError> {
    if let Some(actual) = snapshot.get(field).and_then(JsonValue::as_str) {
        if actual != expected {
            return Err(activation_preflight_error(
                definition_id,
                version,
                field,
                expected,
                actual,
            ));
        }
    }
    Ok(())
}

fn activation_preflight_error(
    definition_id: &str,
    version: u32,
    field: &str,
    expected: &str,
    actual: &str,
) -> CommandError {
    CommandError::user_fixable(
        "agent_definition_activation_preflight_failed",
        format!(
            "Xero cannot start a run from `{definition_id}` version {version} because activation preflight found snapshot field `{field}` = `{actual}`, expected `{expected}`."
        ),
    )
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
        &["draft", "valid", "active", "archived", "blocked"],
    )?;
    validate_known_agent_definition_value(
        "baseCapabilityProfile",
        &record.base_capability_profile,
        &[
            "observe_only",
            "planning",
            "repository_recon",
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
        "planning" => RuntimeAgentIdDto::Plan,
        "repository_recon" => RuntimeAgentIdDto::Crawl,
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

pub fn record_agent_definition_custom_audit_event(
    repo_root: &Path,
    action_kind: &str,
    definition_id: &str,
    version: u32,
    scope: &str,
    lifecycle_state: &str,
    base_capability_profile: &str,
    validation_status: Option<&str>,
    snapshot: Option<&JsonValue>,
    extra_payload: JsonValue,
    created_at: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        action_kind,
        "actionKind",
        "agent_definition_audit_request_invalid",
    )?;
    validate_non_empty_text(
        definition_id,
        "definitionId",
        "agent_definition_audit_request_invalid",
    )?;
    validate_non_empty_text(
        created_at,
        "createdAt",
        "agent_definition_audit_request_invalid",
    )?;
    if version == 0 {
        return Err(CommandError::invalid_request("version"));
    }

    let project_id = project_id_for_agent_definition_audit(repo_root)?;
    let mut payload = JsonMap::new();
    payload.insert("definitionId".into(), json!(definition_id));
    payload.insert("version".into(), json!(version));
    payload.insert("scope".into(), json!(scope));
    payload.insert("lifecycleState".into(), json!(lifecycle_state));
    payload.insert(
        "baseCapabilityProfile".into(),
        json!(base_capability_profile),
    );
    payload.insert(
        "validationStatus".into(),
        validation_status
            .map(|status| JsonValue::String(status.to_string()))
            .unwrap_or(JsonValue::Null),
    );
    payload.insert(
        "attachedSkills".into(),
        attached_skill_audit_summary(snapshot),
    );
    if let Some(extra) = extra_payload.as_object() {
        for (key, value) in extra {
            payload.insert(key.clone(), value.clone());
        }
    }

    record_agent_runtime_audit_event(
        repo_root,
        &NewAgentRuntimeAuditEventRecord {
            audit_id: agent_runtime_audit_id(
                &project_id,
                action_kind,
                "custom_agent",
                definition_id,
                created_at,
            ),
            project_id,
            actor_kind: "user".into(),
            actor_id: None,
            action_kind: action_kind.into(),
            subject_kind: "custom_agent".into(),
            subject_id: definition_id.to_string(),
            run_id: None,
            agent_definition_id: Some(definition_id.to_string()),
            agent_definition_version: Some(version),
            risk_class: capability_permission_explanation("custom_agent", definition_id)
                .get("riskClass")
                .and_then(JsonValue::as_str)
                .map(ToOwned::to_owned),
            approval_action_id: None,
            payload: JsonValue::Object(payload),
            created_at: created_at.to_string(),
        },
    )
    .map(|_| ())
}

fn attached_skill_audit_summary(snapshot: Option<&JsonValue>) -> JsonValue {
    let entries = snapshot
        .and_then(|snapshot| snapshot.get("attachedSkills"))
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(|skill| {
            let object = skill.as_object()?;
            let source_id = object
                .get("sourceId")
                .and_then(JsonValue::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())?;
            Some(json!({
                "id": object.get("id").and_then(JsonValue::as_str).unwrap_or_default(),
                "sourceId": source_id,
                "skillId": object.get("skillId").and_then(JsonValue::as_str).unwrap_or_default(),
                "sourceKind": object.get("sourceKind").and_then(JsonValue::as_str).unwrap_or_default(),
                "scope": object.get("scope").and_then(JsonValue::as_str).unwrap_or_default(),
                "versionHash": object.get("versionHash").and_then(JsonValue::as_str).unwrap_or_default(),
                "includeSupportingAssets": object.get("includeSupportingAssets").and_then(JsonValue::as_bool).unwrap_or(false),
                "required": object.get("required").and_then(JsonValue::as_bool).unwrap_or(false),
            }))
        })
        .collect::<Vec<_>>();
    let source_ids = entries
        .iter()
        .filter_map(|entry| entry.get("sourceId").and_then(JsonValue::as_str))
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    json!({
        "count": entries.len(),
        "sourceIds": source_ids,
        "entries": entries
    })
}

fn project_id_for_agent_definition_audit(repo_root: &Path) -> Result<String, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    connection
        .query_row(
            r#"
            SELECT id
            FROM projects
            ORDER BY id
            LIMIT 1
            "#,
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            map_agent_definition_read_error("agent_definition_audit_project_read_failed", error)
        })?
        .ok_or_else(|| {
            CommandError::retryable(
                "agent_definition_audit_project_missing",
                "Xero could not record an agent-definition audit event because the project row was missing.",
            )
        })
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
                "schema": "xero.agent_definition.v1",
                "schemaVersion": AGENT_DEFINITION_SCHEMA_VERSION,
                "id": "project_researcher",
                "version": version,
                "displayName": "Project Researcher",
                "shortLabel": "Research",
                "description": "Answer project questions using observe-only context.",
                "taskPurpose": "Answer project questions using observe-only context.",
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "observe_only",
                "defaultApprovalMode": "suggest",
                "allowedApprovalModes": ["suggest"],
                "toolPolicy": {
                    "allowedEffectClasses": ["observe"],
                    "allowedTools": ["project_context_search"],
                    "deniedTools": [],
                    "allowedToolGroups": ["project_context"],
                    "deniedToolGroups": []
                },
                "workflowContract": "Use reviewed project context to answer the user's question.",
                "finalResponseContract": "Return a concise answer with uncertainty called out.",
                "prompts": [
                    {
                        "id": "project-researcher-intent",
                        "label": "Project Researcher Intent",
                        "role": "developer",
                        "source": "test",
                        "body": "Answer project questions using only observe-only context."
                    }
                ],
                "tools": [],
                "output": {
                    "contract": "answer",
                    "label": "Answer",
                    "description": "Answer the user's project question.",
                    "sections": [
                        {
                            "id": "answer",
                            "label": "Answer",
                            "description": "Direct answer.",
                            "emphasis": "core",
                            "producedByTools": ["project_context_search"]
                        }
                    ]
                },
                "dbTouchpoints": {
                    "reads": [
                        {
                            "table": "project_records",
                            "kind": "read",
                            "purpose": "Retrieve reviewed project context.",
                            "triggers": [],
                            "columns": ["text"]
                        }
                    ],
                    "writes": [],
                    "encouraged": []
                },
                "consumes": [],
                "projectDataPolicy": {
                    "recordKinds": ["artifact", "context_note"],
                    "structuredSchemas": [],
                    "unstructuredScopes": ["project"]
                },
                "memoryCandidatePolicy": {
                    "memoryKinds": ["project_fact"],
                    "reviewRequired": true
                },
                "retrievalDefaults": {
                    "enabled": true,
                    "limit": 4,
                    "recordKinds": ["artifact", "context_note"],
                    "memoryKinds": ["project_fact"]
                },
                "handoffPolicy": {
                    "enabled": true,
                    "preserveDefinitionVersion": true
                },
                "attachedSkills": []
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
    fn s11_agent_definition_version_diff_is_derived_from_saved_versions() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        create_project_database(&repo_root, "project-definition-diff");
        let mut version_one = custom_definition(1, "2026-05-01T12:01:00Z");
        version_one.snapshot["prompts"] = json!([
            {
                "id": "prompt-v1",
                "label": "Prompt v1",
                "role": "developer",
                "source": "custom",
                "body": "Use the old release-note workflow."
            }
        ]);
        version_one.snapshot["toolPolicy"] = json!({
            "allowedTools": ["project_context_search"],
            "deniedTools": [],
            "allowedEffectClasses": ["observe"]
        });
        version_one.snapshot["outputContract"] = json!("answer");
        version_one.snapshot["dbTouchpoints"] = json!({
            "reads": [{"table": "agent_context_manifests", "purpose": "Read old context."}],
            "writes": [],
            "encouraged": []
        });
        insert_agent_definition(&repo_root, &version_one).expect("insert definition v1");

        let mut version_two = custom_definition(2, "2026-05-01T12:03:00Z");
        version_two.snapshot["prompts"] = json!([
            {
                "id": "prompt-v2",
                "label": "Prompt v2",
                "role": "developer",
                "source": "custom",
                "body": "Use the new support-triage workflow."
            }
        ]);
        version_two.snapshot["toolPolicy"] = json!({
            "allowedTools": ["project_context_search", "agent_memory_search"],
            "deniedTools": ["browser_open"],
            "allowedEffectClasses": ["observe", "runtime_state"]
        });
        version_two.snapshot["memoryPolicy"] = json!({ "reviewRequired": true });
        version_two.snapshot["retrievalDefaults"] = json!({ "enabled": true, "limit": 6 });
        version_two.snapshot["handoffPolicy"] = json!({ "enabled": true });
        version_two.snapshot["outputContract"] = json!("debug_summary");
        version_two.snapshot["dbTouchpoints"] = json!({
            "reads": [{"table": "agent_context_manifests", "purpose": "Read current context."}],
            "writes": [{"table": "agent_runtime_audit_events", "purpose": "Record audit evidence."}],
            "encouraged": []
        });
        version_two.snapshot["consumes"] = json!([
            {
                "id": "plan_pack",
                "label": "Plan Pack",
                "contract": "plan_pack",
                "required": true
            }
        ]);
        version_two.snapshot["attachedSkills"] = json!([
            {
                "id": "rust-best-practices",
                "sourceId": "skill-source:v1:global:bundled:core:rust-best-practices",
                "skillId": "rust-best-practices",
                "name": "Rust Best Practices",
                "description": "Guide for writing idiomatic Rust code.",
                "sourceKind": "bundled",
                "scope": "global",
                "versionHash": "version-hash-rust",
                "includeSupportingAssets": false,
                "required": true
            }
        ]);
        insert_agent_definition(&repo_root, &version_two).expect("insert definition v2");

        let diff = load_agent_definition_version_diff(&repo_root, "project_researcher", 1, 2)
            .expect("load saved-version diff");

        assert_eq!(
            diff["schema"],
            json!("xero.agent_definition_version_diff.v1")
        );
        assert_eq!(diff["definitionId"], json!("project_researcher"));
        assert_eq!(diff["fromVersion"], json!(1));
        assert_eq!(diff["toVersion"], json!(2));
        assert_eq!(diff["changed"], json!(true));
        let changed_sections = diff["changedSections"]
            .as_array()
            .expect("changed sections")
            .iter()
            .filter_map(JsonValue::as_str)
            .collect::<Vec<_>>();
        for expected in [
            "prompts",
            "attachedSkills",
            "toolPolicy",
            "memoryPolicy",
            "retrievalPolicy",
            "handoffPolicy",
            "outputContract",
            "databaseAccess",
            "consumedArtifacts",
        ] {
            assert!(
                changed_sections.contains(&expected),
                "expected changed section `{expected}`"
            );
        }
        let prompts = diff["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .find(|section| section["section"] == json!("prompts"))
            .expect("prompt diff section");
        assert_eq!(prompts["before"]["prompts"][0]["id"], json!("prompt-v1"));
        assert_eq!(prompts["after"]["prompts"][0]["id"], json!("prompt-v2"));
        let attached_skills = diff["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .find(|section| section["section"] == json!("attachedSkills"))
            .expect("attached skills diff section");
        assert_eq!(
            attached_skills["after"]["attachedSkills"][0]["sourceId"],
            json!("skill-source:v1:global:bundled:core:rust-best-practices")
        );
        let db_access = diff["sections"]
            .as_array()
            .expect("sections")
            .iter()
            .find(|section| section["section"] == json!("databaseAccess"))
            .expect("database diff section");
        assert_eq!(
            db_access["after"]["dbTouchpoints"]["writes"][0]["table"],
            json!("agent_runtime_audit_events")
        );
    }

    #[test]
    fn s7_agent_definition_audit_payloads_include_redacted_attached_skill_sources() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        let project_id = "project-definition-attached-skill-audit";
        create_project_database(&repo_root, project_id);
        let mut definition = custom_definition(1, "2026-05-01T12:01:00Z");
        definition.snapshot["attachedSkills"] = json!([
            {
                "id": "rust-best-practices",
                "sourceId": "skill-source:v1:global:bundled:core:rust-best-practices",
                "skillId": "rust-best-practices",
                "name": "Rust Best Practices",
                "description": "Guide for writing idiomatic Rust code.",
                "sourceKind": "bundled",
                "scope": "global",
                "versionHash": "version-hash-rust",
                "includeSupportingAssets": false,
                "required": true
            }
        ]);

        insert_agent_definition(&repo_root, &definition).expect("insert attached skill definition");
        archive_agent_definition(&repo_root, "project_researcher", "2026-05-01T12:05:00Z")
            .expect("archive attached skill definition");

        let audit_events = project_store::list_agent_runtime_audit_events_for_subject(
            &repo_root,
            project_id,
            "custom_agent",
            "project_researcher",
        )
        .expect("list custom agent audit events");
        let saved = audit_events
            .iter()
            .find(|event| event.action_kind == "agent_definition_saved")
            .expect("saved audit event");
        let archived = audit_events
            .iter()
            .find(|event| event.action_kind == "agent_definition_archived")
            .expect("archived audit event");
        for event in [saved, archived] {
            assert_eq!(event.payload["attachedSkills"]["count"], json!(1));
            assert_eq!(
                event.payload["attachedSkills"]["sourceIds"][0],
                json!("skill-source:v1:global:bundled:core:rust-best-practices")
            );
            assert_eq!(
                event.payload["attachedSkills"]["entries"][0]["versionHash"],
                json!("version-hash-rust")
            );
            assert!(
                !serde_json::to_string(&event.payload)
                    .expect("audit payload json")
                    .contains("/"),
                "attached skill audit payload must not expose local paths"
            );
        }
    }

    #[test]
    fn s17_custom_definition_activation_requires_valid_validation_report() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        create_project_database(&repo_root, "project-invalid-custom-definition");
        let mut definition = custom_definition(1, "2026-05-01T12:01:00Z");
        definition.validation_report = Some(json!({
            "status": "invalid",
            "diagnostics": [
                {
                    "code": "missing_output_contract",
                    "severity": "error",
                    "message": "Output contract is required."
                }
            ]
        }));

        insert_agent_definition(&repo_root, &definition).expect("insert invalid custom definition");
        let error = resolve_agent_definition_for_run(
            &repo_root,
            Some("project_researcher"),
            RuntimeAgentIdDto::Ask,
        )
        .expect_err("invalid validation report blocks activation");

        assert_eq!(error.code, "agent_definition_activation_preflight_failed");
        assert!(error
            .message
            .contains("does not have a valid custom-agent validation report"));
    }

    #[test]
    fn s17_custom_definition_activation_rejects_snapshot_profile_mismatch() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        create_project_database(&repo_root, "project-mismatched-custom-definition");
        let mut definition = custom_definition(1, "2026-05-01T12:01:00Z");
        definition.snapshot["baseCapabilityProfile"] = json!("engineering");

        insert_agent_definition(&repo_root, &definition)
            .expect("insert mismatched custom definition");
        let error = resolve_agent_definition_for_run(
            &repo_root,
            Some("project_researcher"),
            RuntimeAgentIdDto::Ask,
        )
        .expect_err("mismatched snapshot profile blocks activation");

        assert_eq!(error.code, "agent_definition_activation_preflight_failed");
        assert!(error.message.contains("baseCapabilityProfile"));
        assert!(error.message.contains("engineering"));
        assert!(error.message.contains("observe_only"));
    }

    #[test]
    fn s17_custom_definition_activation_rejects_stale_valid_report_with_missing_runtime_policy() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        create_project_database(&repo_root, "project-stale-valid-custom-definition");
        let mut definition = custom_definition(1, "2026-05-01T12:01:00Z");
        definition
            .snapshot
            .as_object_mut()
            .expect("snapshot object")
            .remove("handoffPolicy");

        insert_agent_definition(&repo_root, &definition)
            .expect("insert definition with stale valid report");
        let preflight = load_agent_activation_preflight(&repo_root, "project_researcher")
            .expect("load activation preflight report");
        assert_eq!(
            preflight["schema"],
            json!("xero.custom_agent_activation_preflight.v1")
        );
        assert_eq!(preflight["status"], json!("failed"));
        let context_check = preflight["checks"]
            .as_array()
            .expect("preflight checks")
            .iter()
            .find(|check| check["id"] == json!("context_policy"))
            .expect("context policy check");
        assert_eq!(context_check["status"], json!("failed"));
        assert!(context_check["summary"]
            .as_str()
            .expect("context summary")
            .contains("handoffPolicy"));

        let error = resolve_agent_definition_for_run(
            &repo_root,
            Some("project_researcher"),
            RuntimeAgentIdDto::Ask,
        )
        .expect_err("runtime preflight blocks stale valid report");

        assert_eq!(error.code, "agent_definition_activation_preflight_failed");
        assert!(error.message.contains("context_policy"));
        assert!(error.message.contains("handoffPolicy"));
    }

    #[test]
    fn s19_non_active_custom_definitions_are_not_normal_run_candidates() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        create_project_database(&repo_root, "project-blocked-custom-definition");
        let mut draft = custom_definition(1, "2026-05-01T12:00:00Z");
        draft.definition_id = "draft_project_researcher".into();
        draft.lifecycle_state = "draft".into();
        draft.snapshot["id"] = json!("draft_project_researcher");
        draft.snapshot["lifecycleState"] = json!("draft");
        insert_agent_definition(&repo_root, &draft).expect("insert draft definition");
        let mut valid = custom_definition(1, "2026-05-01T12:00:30Z");
        valid.definition_id = "valid_project_researcher".into();
        valid.lifecycle_state = "valid".into();
        valid.snapshot["id"] = json!("valid_project_researcher");
        valid.snapshot["lifecycleState"] = json!("valid");
        insert_agent_definition(&repo_root, &valid).expect("insert valid definition");
        let mut definition = custom_definition(1, "2026-05-01T12:01:00Z");
        definition.lifecycle_state = "blocked".into();
        definition.snapshot["lifecycleState"] = json!("blocked");

        insert_agent_definition(&repo_root, &definition).expect("insert blocked definition");
        let normal_list =
            list_agent_definitions(&repo_root, false).expect("list normal definitions");
        for hidden_id in [
            "draft_project_researcher",
            "valid_project_researcher",
            "project_researcher",
        ] {
            assert!(!normal_list
                .iter()
                .any(|record| record.definition_id == hidden_id));
        }
        let full_list = list_agent_definitions(&repo_root, true).expect("list all definitions");
        for (definition_id, lifecycle_state) in [
            ("draft_project_researcher", "draft"),
            ("valid_project_researcher", "valid"),
            ("project_researcher", "blocked"),
        ] {
            assert!(full_list.iter().any(|record| {
                record.definition_id == definition_id && record.lifecycle_state == lifecycle_state
            }));
            let error = resolve_agent_definition_for_run(
                &repo_root,
                Some(definition_id),
                RuntimeAgentIdDto::Ask,
            )
            .expect_err("non-active definition cannot start a run");
            assert_eq!(error.code, "agent_definition_inactive");
            assert!(error.message.contains(lifecycle_state));
        }
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
