use std::{collections::HashSet, path::Path};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use xero_agent_core::domain_tool_pack_manifest;

use crate::{commands::CommandError, db::database_path_for_repo};

use super::{
    get_agent_handoff_lineage_by_handoff_id, list_agent_context_manifests_for_run,
    list_agent_handoff_lineage_by_status, list_approved_agent_memories, list_project_records,
    load_agent_definition_version, load_agent_run, open_runtime_database,
    project_record_kind_sql_value, read_project_row, validate_non_empty_text,
    AgentHandoffLineageRecord, AgentHandoffLineageStatus, AgentMemoryKind, AgentMemoryRecord,
    AgentMemoryReviewState, ProjectRecordImportance, ProjectRecordRecord,
    ProjectRecordRedactionState, ProjectRecordVisibility,
};

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRuntimeAuditEventRecord {
    pub id: i64,
    pub audit_id: String,
    pub project_id: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub action_kind: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub run_id: Option<String>,
    pub agent_definition_id: Option<String>,
    pub agent_definition_version: Option<u32>,
    pub risk_class: Option<String>,
    pub approval_action_id: Option<String>,
    pub payload: JsonValue,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentRuntimeAuditEventRecord {
    pub audit_id: String,
    pub project_id: String,
    pub actor_kind: String,
    pub actor_id: Option<String>,
    pub action_kind: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub run_id: Option<String>,
    pub agent_definition_id: Option<String>,
    pub agent_definition_version: Option<u32>,
    pub risk_class: Option<String>,
    pub approval_action_id: Option<String>,
    pub payload: JsonValue,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentCapabilityRevocationRecord {
    pub id: i64,
    pub revocation_id: String,
    pub project_id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub scope: JsonValue,
    pub reason: String,
    pub created_by: String,
    pub status: String,
    pub created_at: String,
    pub cleared_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentCapabilityRevocationRecord {
    pub revocation_id: String,
    pub project_id: String,
    pub subject_kind: String,
    pub subject_id: String,
    pub scope: JsonValue,
    pub reason: String,
    pub created_by: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRuntimeAuditExport {
    pub project_id: String,
    pub run_id: String,
    pub agent_session_id: String,
    pub runtime_agent_id: String,
    pub provider_id: String,
    pub model_id: String,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub context_manifest_ids: Vec<String>,
    pub effective_prompt_sections: JsonValue,
    pub tool_policy: JsonValue,
    pub memory_policy: JsonValue,
    pub retrieval_policy: JsonValue,
    pub output_contract: JsonValue,
    pub handoff_policy: JsonValue,
    pub capability_permission_explanations: Vec<JsonValue>,
    pub risky_capability_approvals: Vec<AgentRuntimeAuditEventRecord>,
    pub audit_events: Vec<AgentRuntimeAuditEventRecord>,
}

pub fn agent_runtime_audit_id(
    project_id: &str,
    action_kind: &str,
    subject_kind: &str,
    subject_id: &str,
    created_at: &str,
) -> String {
    short_stable_id(
        "agent-audit",
        &[
            project_id,
            action_kind,
            subject_kind,
            subject_id,
            created_at,
        ],
    )
}

pub fn agent_capability_revocation_id(
    project_id: &str,
    subject_kind: &str,
    subject_id: &str,
) -> String {
    short_stable_id("agent-revocation", &[project_id, subject_kind, subject_id])
}

pub fn capability_permission_explanation(subject_kind: &str, subject_id: &str) -> JsonValue {
    let (summary, data_access, network_access, file_mutation, confirmation_required, risk_class) =
        match subject_kind {
            "custom_agent" => (
                "Custom agent definition can select prompts, policies, tool packs, memory, retrieval, and handoff behavior for new runs.",
                "project runtime state, selected project context, approved memory, and any tools granted by its effective policy",
                "depends_on_effective_tool_policy",
                "depends_on_effective_tool_policy",
                true,
                "custom_agent_runtime",
            ),
            "tool_pack" => (
                "Tool pack grants expose a group of runtime tools to an agent.",
                "data reachable by tools inside the pack",
                "depends_on_tools_in_pack",
                "depends_on_tools_in_pack",
                true,
                "tool_pack_grant",
            ),
            "external_integration" => (
                "External integration grants may allow the runtime to call a service outside the local project.",
                "integration-specific account or service data",
                "can_leave_local_machine",
                "integration_specific",
                true,
                "external_capability",
            ),
            "browser_control" => (
                "Browser-control grants may let the runtime inspect or operate a browser session.",
                "browser page content, profile state, and user-visible web data",
                "can_leave_local_machine",
                "can_mutate_remote_or_browser_state",
                true,
                "browser_control",
            ),
            "destructive_write" => (
                "Destructive-write grants may allow irreversible file, repository, or project-state mutations.",
                "repo files and project state targeted by the approved operation",
                "local_by_default",
                "can_delete_or_overwrite",
                true,
                "destructive_write",
            ),
            _ => (
                "Unknown capability grant.",
                "unknown",
                "unknown",
                "unknown",
                true,
                "unknown",
            ),
        };
    let mut explanation = json!({
        "schema": "xero.capability_permission_explanation.v1",
        "subjectKind": subject_kind,
        "subjectId": subject_id,
        "summary": summary,
        "dataAccess": data_access,
        "networkAccess": network_access,
        "fileMutation": file_mutation,
        "confirmationRequired": confirmation_required,
        "riskClass": risk_class,
    });
    if subject_kind == "tool_pack" {
        if let Some(manifest) = domain_tool_pack_manifest(subject_id) {
            explanation["summary"] = json!(format!(
                "Tool pack `{}` grants {} tool(s) for {}.",
                manifest.label,
                manifest.tools.len(),
                manifest.summary
            ));
            explanation["dataAccess"] = json!(format!(
                "data reachable by tools in groups: {}",
                manifest.tool_groups.join(", ")
            ));
            explanation["networkAccess"] = json!(tool_pack_network_access(&manifest));
            explanation["fileMutation"] = json!(tool_pack_file_mutation(&manifest));
            explanation["toolPack"] = json!({
                "packId": manifest.pack_id,
                "label": manifest.label,
                "policyProfile": manifest.policy_profile,
                "tools": manifest.tools,
                "capabilities": manifest.capabilities,
                "allowedEffectClasses": manifest.allowed_effect_classes,
                "deniedEffectClasses": manifest.denied_effect_classes,
                "reviewRequirements": manifest.review_requirements,
                "approvalBoundaries": manifest.approval_boundaries,
            });
        }
    }
    explanation
}

pub fn load_agent_database_touchpoint_explanation(
    repo_root: &Path,
    project_id: &str,
    definition_id: &str,
    version: u32,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_db_touchpoint_explanation_project_required",
    )?;
    validate_non_empty_text(
        definition_id,
        "definitionId",
        "agent_db_touchpoint_explanation_definition_required",
    )?;
    if version == 0 {
        return Err(CommandError::invalid_request("version"));
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let definition =
        load_agent_definition_version(repo_root, definition_id, version)?.ok_or_else(|| {
            CommandError::user_fixable(
                "agent_definition_version_missing",
                format!(
                    "Xero could not find version {version} of agent definition `{definition_id}`."
                ),
            )
        })?;

    Ok(agent_database_touchpoint_explanation_json(
        project_id,
        definition_id,
        version,
        &definition.snapshot,
    ))
}

fn agent_database_touchpoint_explanation_json(
    project_id: &str,
    definition_id: &str,
    version: u32,
    snapshot: &JsonValue,
) -> JsonValue {
    let touchpoints = snapshot.get("dbTouchpoints");
    let reads = database_touchpoint_entries(touchpoints, "reads", "read");
    let writes = database_touchpoint_entries(touchpoints, "writes", "write");
    let encouraged = database_touchpoint_entries(touchpoints, "encouraged", "encouraged");
    let read_count = reads.len();
    let write_count = writes.len();
    let encouraged_count = encouraged.len();

    json!({
        "schema": "xero.agent_database_touchpoint_explanation.v1",
        "projectId": project_id,
        "definition": {
            "definitionId": definition_id,
            "version": version,
        },
        "summary": {
            "readCount": read_count,
            "writeCount": write_count,
            "encouragedCount": encouraged_count,
            "hasWrites": write_count > 0,
        },
        "touchpoints": {
            "reads": reads,
            "writes": writes,
            "encouraged": encouraged,
        },
        "explanation": {
            "readBehavior": "Read touchpoints guide context selection and tell the runtime which app-data tables are relevant to this agent.",
            "writeBehavior": if write_count > 0 {
                "Write touchpoints identify project-state tables this saved agent definition may intentionally change when its effective tool policy also permits mutation."
            } else {
                "This saved agent definition declares no write touchpoints."
            },
            "encouragedBehavior": "Encouraged touchpoints are lower-priority hints; they do not grant database access by themselves.",
            "auditVisibility": "Provider context manifests and runtime audit records carry the compact touchpoint summary under agentDefinition.dbTouchpoints.",
            "userConfirmation": "Database touchpoints do not bypass tool policy or approval gates; mutating tools still require the effective runtime permission and any configured confirmation."
        },
        "source": {
            "kind": "agent_definition_snapshot",
            "path": "dbTouchpoints",
        },
        "uiDeferred": true,
    })
}

fn database_touchpoint_entries(
    touchpoints: Option<&JsonValue>,
    field: &str,
    kind: &str,
) -> Vec<JsonValue> {
    touchpoints
        .and_then(|value| value.get(field))
        .and_then(JsonValue::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| {
                    let object = entry.as_object()?;
                    Some(json!({
                        "table": object
                            .get("table")
                            .and_then(JsonValue::as_str)
                            .unwrap_or("unknown"),
                        "kind": object
                            .get("kind")
                            .and_then(JsonValue::as_str)
                            .unwrap_or(kind),
                        "purpose": object
                            .get("purpose")
                            .and_then(JsonValue::as_str)
                            .unwrap_or("No purpose recorded."),
                        "columns": string_array_field(entry, "columns"),
                        "triggerCount": object
                            .get("triggers")
                            .and_then(JsonValue::as_array)
                            .map_or(0, Vec::len),
                    }))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn string_array_field(value: &JsonValue, field: &str) -> Vec<String> {
    value
        .get(field)
        .and_then(JsonValue::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(JsonValue::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn tool_pack_network_access(manifest: &xero_agent_core::DomainToolPackManifest) -> &'static str {
    if manifest
        .allowed_effect_classes
        .iter()
        .any(|effect| matches!(effect.as_str(), "external_service" | "browser_control"))
    {
        "can_leave_local_machine"
    } else {
        "local_by_default"
    }
}

fn tool_pack_file_mutation(manifest: &xero_agent_core::DomainToolPackManifest) -> &'static str {
    if manifest
        .allowed_effect_classes
        .iter()
        .any(|effect| matches!(effect.as_str(), "destructive_write" | "write" | "command"))
    {
        "can_mutate_files_or_project_state"
    } else if manifest
        .allowed_effect_classes
        .iter()
        .any(|effect| effect == "runtime_state")
    {
        "can_mutate_project_state"
    } else {
        "no_direct_file_mutation_declared"
    }
}

pub fn record_agent_runtime_audit_event(
    repo_root: &Path,
    record: &NewAgentRuntimeAuditEventRecord,
) -> Result<AgentRuntimeAuditEventRecord, CommandError> {
    validate_new_audit_event(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    insert_audit_event(&connection, record)?;
    load_agent_runtime_audit_event(repo_root, &record.project_id, &record.audit_id)?.ok_or_else(
        || {
            CommandError::system_fault(
                "agent_runtime_audit_event_missing",
                "Xero recorded an agent runtime audit event but could not load it back.",
            )
        },
    )
}

pub fn load_agent_runtime_audit_event(
    repo_root: &Path,
    project_id: &str,
    audit_id: &str,
) -> Result<Option<AgentRuntimeAuditEventRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_runtime_audit_project_required",
    )?;
    validate_non_empty_text(audit_id, "auditId", "agent_runtime_audit_id_required")?;
    let connection = open_runtime_database(repo_root, &database_path_for_repo(repo_root))?;
    connection
        .query_row(
            audit_select_sql("WHERE project_id = ?1 AND audit_id = ?2").as_str(),
            params![project_id, audit_id],
            read_audit_row,
        )
        .optional()
        .map_err(map_audit_read_error)?
        .transpose()
}

pub fn list_agent_runtime_audit_events_for_subject(
    repo_root: &Path,
    project_id: &str,
    subject_kind: &str,
    subject_id: &str,
) -> Result<Vec<AgentRuntimeAuditEventRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_runtime_audit_project_required",
    )?;
    validate_subject_kind(subject_kind)?;
    validate_non_empty_text(
        subject_id,
        "subjectId",
        "agent_runtime_audit_subject_required",
    )?;
    let connection = open_runtime_database(repo_root, &database_path_for_repo(repo_root))?;
    let mut statement = connection
        .prepare(
            audit_select_sql(
                r#"
                WHERE project_id = ?1
                  AND subject_kind = ?2
                  AND subject_id = ?3
                ORDER BY created_at ASC, id ASC
                "#,
            )
            .as_str(),
        )
        .map_err(map_audit_read_error)?;
    let rows = statement
        .query_map(
            params![project_id, subject_kind, subject_id],
            read_audit_row,
        )
        .map_err(map_audit_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_audit_read_error)?
        .into_iter()
        .collect()
}

pub fn list_agent_runtime_audit_events_for_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AgentRuntimeAuditEventRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_runtime_audit_project_required",
    )?;
    validate_non_empty_text(run_id, "runId", "agent_runtime_audit_run_required")?;
    let connection = open_runtime_database(repo_root, &database_path_for_repo(repo_root))?;
    let mut statement = connection
        .prepare(
            audit_select_sql(
                r#"
                WHERE project_id = ?1
                  AND run_id = ?2
                ORDER BY created_at ASC, id ASC
                "#,
            )
            .as_str(),
        )
        .map_err(map_audit_read_error)?;
    let rows = statement
        .query_map(params![project_id, run_id], read_audit_row)
        .map_err(map_audit_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_audit_read_error)?
        .into_iter()
        .collect()
}

pub fn revoke_agent_capability(
    repo_root: &Path,
    record: &NewAgentCapabilityRevocationRecord,
) -> Result<AgentCapabilityRevocationRecord, CommandError> {
    validate_new_revocation(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::retryable(
            "agent_capability_revocation_transaction_failed",
            format!(
                "Xero could not start a capability-revocation transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;
    if record.subject_kind == "custom_agent" {
        block_custom_agent_definition(&transaction, record)?;
    }
    let scope_json = serde_json::to_string(&record.scope).map_err(|error| {
        CommandError::system_fault(
            "agent_capability_revocation_scope_encode_failed",
            format!("Xero could not encode capability revocation scope: {error}"),
        )
    })?;
    transaction
        .execute(
            r#"
            INSERT INTO agent_capability_revocations (
                revocation_id,
                project_id,
                subject_kind,
                subject_id,
                scope_json,
                reason,
                created_by,
                status,
                created_at,
                cleared_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'active', ?8, NULL)
            ON CONFLICT(project_id, subject_kind, subject_id)
            WHERE status = 'active'
            DO UPDATE SET
                scope_json = excluded.scope_json,
                reason = excluded.reason,
                created_by = excluded.created_by,
                created_at = excluded.created_at,
                cleared_at = NULL
            "#,
            params![
                record.revocation_id,
                record.project_id,
                record.subject_kind,
                record.subject_id,
                scope_json,
                record.reason,
                record.created_by,
                record.created_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_capability_revocation_failed",
                format!(
                    "Xero could not persist capability revocation `{}` in {}: {error}",
                    record.revocation_id,
                    database_path.display()
                ),
            )
        })?;
    let audit = NewAgentRuntimeAuditEventRecord {
        audit_id: agent_runtime_audit_id(
            &record.project_id,
            "capability_revoked",
            &record.subject_kind,
            &record.subject_id,
            &record.created_at,
        ),
        project_id: record.project_id.clone(),
        actor_kind: "user".into(),
        actor_id: Some(record.created_by.clone()),
        action_kind: "capability_revoked".into(),
        subject_kind: record.subject_kind.clone(),
        subject_id: record.subject_id.clone(),
        run_id: None,
        agent_definition_id: if record.subject_kind == "custom_agent" {
            Some(record.subject_id.clone())
        } else {
            None
        },
        agent_definition_version: None,
        risk_class: capability_permission_explanation(&record.subject_kind, &record.subject_id)
            .get("riskClass")
            .and_then(JsonValue::as_str)
            .map(ToOwned::to_owned),
        approval_action_id: None,
        payload: json!({
            "revocationId": record.revocation_id,
            "reason": record.reason,
            "permission": capability_permission_explanation(&record.subject_kind, &record.subject_id),
        }),
        created_at: record.created_at.clone(),
    };
    insert_audit_event(&transaction, &audit)?;
    transaction.commit().map_err(|error| {
        CommandError::retryable(
            "agent_capability_revocation_commit_failed",
            format!(
                "Xero could not commit capability revocation `{}` in {}: {error}",
                record.revocation_id,
                database_path.display()
            ),
        )
    })?;
    load_agent_capability_revocation(repo_root, &record.project_id, &record.revocation_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "agent_capability_revocation_missing",
                "Xero recorded a capability revocation but could not load it back.",
            )
        })
}

pub fn clear_agent_capability_revocation(
    repo_root: &Path,
    project_id: &str,
    revocation_id: &str,
    cleared_at: &str,
) -> Result<AgentCapabilityRevocationRecord, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_capability_revocation_project_required",
    )?;
    validate_non_empty_text(
        revocation_id,
        "revocationId",
        "agent_capability_revocation_id_required",
    )?;
    validate_non_empty_text(
        cleared_at,
        "clearedAt",
        "agent_capability_revocation_cleared_at_required",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let transaction = connection.unchecked_transaction().map_err(|error| {
        CommandError::retryable(
            "agent_capability_revocation_transaction_failed",
            format!(
                "Xero could not start a capability-revocation transaction in {}: {error}",
                database_path.display()
            ),
        )
    })?;
    let existing = read_revocation_by_id(&transaction, project_id, revocation_id)?;
    transaction
        .execute(
            r#"
            UPDATE agent_capability_revocations
            SET status = 'cleared',
                cleared_at = ?3
            WHERE project_id = ?1
              AND revocation_id = ?2
            "#,
            params![project_id, revocation_id, cleared_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_capability_revocation_clear_failed",
                format!(
                    "Xero could not clear capability revocation `{revocation_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let audit = NewAgentRuntimeAuditEventRecord {
        audit_id: agent_runtime_audit_id(
            project_id,
            "capability_revocation_cleared",
            &existing.subject_kind,
            &existing.subject_id,
            cleared_at,
        ),
        project_id: project_id.to_string(),
        actor_kind: "user".into(),
        actor_id: None,
        action_kind: "capability_revocation_cleared".into(),
        subject_kind: existing.subject_kind,
        subject_id: existing.subject_id,
        run_id: None,
        agent_definition_id: None,
        agent_definition_version: None,
        risk_class: None,
        approval_action_id: None,
        payload: json!({ "revocationId": revocation_id }),
        created_at: cleared_at.to_string(),
    };
    insert_audit_event(&transaction, &audit)?;
    transaction.commit().map_err(|error| {
        CommandError::retryable(
            "agent_capability_revocation_clear_commit_failed",
            format!(
                "Xero could not commit capability revocation clear `{revocation_id}` in {}: {error}",
                database_path.display()
            ),
        )
    })?;
    load_agent_capability_revocation(repo_root, project_id, revocation_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "agent_capability_revocation_missing",
            "Xero cleared a capability revocation but could not load it back.",
        )
    })
}

pub fn ensure_agent_capability_not_revoked(
    repo_root: &Path,
    project_id: &str,
    subject_kind: &str,
    subject_id: &str,
) -> Result<(), CommandError> {
    if let Some(revocation) =
        load_active_agent_capability_revocation(repo_root, project_id, subject_kind, subject_id)?
    {
        return Err(CommandError::user_fixable(
            "agent_capability_revoked",
            format!(
                "Xero cannot use `{}` `{}` because revocation `{}` is active: {}",
                revocation.subject_kind,
                revocation.subject_id,
                revocation.revocation_id,
                revocation.reason
            ),
        ));
    }
    Ok(())
}

pub fn load_agent_capability_revocation(
    repo_root: &Path,
    project_id: &str,
    revocation_id: &str,
) -> Result<Option<AgentCapabilityRevocationRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_capability_revocation_project_required",
    )?;
    validate_non_empty_text(
        revocation_id,
        "revocationId",
        "agent_capability_revocation_id_required",
    )?;
    let connection = open_runtime_database(repo_root, &database_path_for_repo(repo_root))?;
    connection
        .query_row(
            revocation_select_sql("WHERE project_id = ?1 AND revocation_id = ?2").as_str(),
            params![project_id, revocation_id],
            read_revocation_row,
        )
        .optional()
        .map_err(map_audit_read_error)?
        .transpose()
}

pub fn load_active_agent_capability_revocation(
    repo_root: &Path,
    project_id: &str,
    subject_kind: &str,
    subject_id: &str,
) -> Result<Option<AgentCapabilityRevocationRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_capability_revocation_project_required",
    )?;
    validate_subject_kind(subject_kind)?;
    validate_non_empty_text(
        subject_id,
        "subjectId",
        "agent_capability_revocation_subject_required",
    )?;
    let connection = open_runtime_database(repo_root, &database_path_for_repo(repo_root))?;
    connection
        .query_row(
            revocation_select_sql(
                "WHERE project_id = ?1 AND subject_kind = ?2 AND subject_id = ?3 AND status = 'active'",
            )
            .as_str(),
            params![project_id, subject_kind, subject_id],
            read_revocation_row,
        )
        .optional()
        .map_err(map_audit_read_error)?
        .transpose()
}

pub fn list_active_agent_capability_revocations(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<AgentCapabilityRevocationRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_capability_revocation_project_required",
    )?;
    let connection = open_runtime_database(repo_root, &database_path_for_repo(repo_root))?;
    let mut statement = connection
        .prepare(
            revocation_select_sql(
                "WHERE project_id = ?1 AND status = 'active' ORDER BY created_at DESC, id DESC",
            )
            .as_str(),
        )
        .map_err(map_audit_read_error)?;
    let rows = statement
        .query_map(params![project_id], read_revocation_row)
        .map_err(map_audit_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_audit_read_error)?
        .into_iter()
        .collect()
}

pub fn export_agent_runtime_audit(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<AgentRuntimeAuditExport, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_runtime_audit_project_required",
    )?;
    validate_non_empty_text(run_id, "runId", "agent_runtime_audit_run_required")?;
    let snapshot = load_agent_run(repo_root, project_id, run_id)?;
    let manifests = list_agent_context_manifests_for_run(repo_root, project_id, run_id)?;
    let latest_manifest = manifests.last();
    let definition = load_agent_definition_version(
        repo_root,
        &snapshot.run.agent_definition_id,
        snapshot.run.agent_definition_version,
    )?;
    let definition_snapshot = definition
        .as_ref()
        .map(|version| version.snapshot.clone())
        .unwrap_or_else(|| json!({}));
    let subject_audit_events = list_agent_runtime_audit_events_for_subject(
        repo_root,
        project_id,
        "custom_agent",
        &snapshot.run.agent_definition_id,
    )
    .unwrap_or_default();
    let run_audit_events =
        list_agent_runtime_audit_events_for_run(repo_root, project_id, run_id).unwrap_or_default();
    let audit_events = merge_audit_events(subject_audit_events, run_audit_events);
    let capability_permission_explanations =
        audit_export_permission_explanations(&snapshot.run.agent_definition_id, &audit_events);
    let risky_capability_approvals = audit_events
        .iter()
        .filter(|event| {
            event.risk_class.is_some()
                || event.approval_action_id.is_some()
                || event.action_kind.contains("approval")
        })
        .cloned()
        .collect::<Vec<_>>();

    Ok(AgentRuntimeAuditExport {
        project_id: project_id.to_string(),
        run_id: run_id.to_string(),
        agent_session_id: snapshot.run.agent_session_id,
        runtime_agent_id: snapshot.run.runtime_agent_id.as_str().into(),
        provider_id: snapshot.run.provider_id,
        model_id: snapshot.run.model_id,
        agent_definition_id: snapshot.run.agent_definition_id,
        agent_definition_version: snapshot.run.agent_definition_version,
        context_manifest_ids: manifests
            .iter()
            .map(|manifest| manifest.manifest_id.clone())
            .collect(),
        effective_prompt_sections: latest_manifest
            .map(|manifest| manifest.manifest["promptFragments"].clone())
            .unwrap_or_else(|| json!([])),
        tool_policy: latest_manifest
            .map(|manifest| {
                json!({
                    "toolDescriptors": manifest.manifest["toolDescriptors"].clone(),
                    "toolExposurePlan": manifest.manifest["toolExposurePlan"].clone(),
                })
            })
            .unwrap_or_else(|| json!({})),
        memory_policy: definition_snapshot
            .get("memoryPolicy")
            .cloned()
            .unwrap_or_else(|| json!({ "status": "not_configured" })),
        retrieval_policy: latest_manifest
            .map(|manifest| manifest.manifest["retrieval"].clone())
            .unwrap_or_else(|| {
                definition_snapshot
                    .get("retrievalDefaults")
                    .cloned()
                    .unwrap_or_else(|| json!({ "status": "not_configured" }))
            }),
        output_contract: definition_snapshot
            .get("outputContract")
            .cloned()
            .unwrap_or_else(|| json!("unknown")),
        handoff_policy: latest_manifest
            .and_then(|manifest| manifest.manifest.get("handoff").cloned())
            .or_else(|| definition_snapshot.get("handoffPolicy").cloned())
            .unwrap_or_else(|| json!({ "status": "not_configured" })),
        capability_permission_explanations,
        risky_capability_approvals,
        audit_events,
    })
}

pub fn load_agent_run_start_explanation(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_run_start_explanation_project_required",
    )?;
    validate_non_empty_text(run_id, "runId", "agent_run_start_explanation_run_required")?;
    let export = export_agent_runtime_audit(repo_root, project_id, run_id)?;
    let manifests = list_agent_context_manifests_for_run(repo_root, project_id, run_id)?;
    let latest_manifest = manifests.last();
    let definition = load_agent_definition_version(
        repo_root,
        &export.agent_definition_id,
        export.agent_definition_version,
    )?;
    let definition_snapshot = definition
        .as_ref()
        .map(|version| version.snapshot.clone())
        .unwrap_or_else(|| json!({}));
    let database_touchpoint_explanation = definition.as_ref().map(|version| {
        agent_database_touchpoint_explanation_json(
            project_id,
            &export.agent_definition_id,
            export.agent_definition_version,
            &version.snapshot,
        )
    });

    let mut explanation = json!({
        "schema": "xero.agent_run_start_explanation.v1",
        "projectId": export.project_id,
        "runId": export.run_id,
        "definition": {
            "runtimeAgentId": export.runtime_agent_id,
            "definitionId": export.agent_definition_id,
            "version": export.agent_definition_version,
        },
        "model": {
            "providerId": export.provider_id,
            "modelId": export.model_id,
        },
        "approval": {
            "defaultMode": definition_snapshot
                .get("defaultApprovalMode")
                .cloned()
                .unwrap_or(JsonValue::Null),
            "allowedModes": definition_snapshot
                .get("allowedApprovalModes")
                .cloned()
                .unwrap_or_else(|| json!([])),
            "source": "agent_definition_snapshot",
        },
        "contextPolicy": latest_manifest
            .and_then(|manifest| manifest.manifest.get("policy").cloned())
            .unwrap_or_else(|| json!({ "status": "not_recorded" })),
        "toolPolicy": export.tool_policy,
        "memoryPolicy": export.memory_policy,
        "retrievalPolicy": export.retrieval_policy,
        "outputContract": export.output_contract,
        "handoffPolicy": export.handoff_policy,
        "capabilityPermissionExplanations": export.capability_permission_explanations,
        "riskyCapabilityApprovalCount": export.risky_capability_approvals.len(),
        "source": {
            "kind": "runtime_audit_export",
            "agentSessionId": export.agent_session_id,
            "contextManifestIds": export.context_manifest_ids,
        },
    });
    if let Some(database_touchpoint_explanation) = database_touchpoint_explanation {
        explanation["databaseTouchpointExplanation"] = database_touchpoint_explanation;
    }
    Ok(explanation)
}

pub fn load_agent_knowledge_inspection(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: Option<&str>,
    run_id: Option<&str>,
    limit: usize,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_knowledge_inspection_project_required",
    )?;
    if let Some(agent_session_id) = agent_session_id {
        validate_non_empty_text(
            agent_session_id,
            "agentSessionId",
            "agent_knowledge_inspection_session_required",
        )?;
    }
    if let Some(run_id) = run_id {
        validate_non_empty_text(run_id, "runId", "agent_knowledge_inspection_run_required")?;
    }
    let limit = limit.clamp(1, 50);
    let runtime_audit = match run_id {
        Some(run_id) => Some(export_agent_runtime_audit(repo_root, project_id, run_id)?),
        None => None,
    };
    let effective_agent_session_id = match (agent_session_id, runtime_audit.as_ref()) {
        (Some(requested), Some(export)) if requested != export.agent_session_id => {
            return Err(CommandError::user_fixable(
                "agent_knowledge_inspection_session_run_mismatch",
                format!(
                    "Xero could not inspect run `{}` with agent session `{requested}` because the run belongs to session `{}`.",
                    export.run_id, export.agent_session_id
                ),
            ))
        }
        (Some(requested), _) => Some(requested.to_string()),
        (None, Some(export)) => Some(export.agent_session_id.clone()),
        (None, None) => None,
    };
    let retrieval_policy = runtime_audit
        .as_ref()
        .map(|export| export.retrieval_policy.clone())
        .unwrap_or_else(|| json!({ "status": "not_requested" }));
    let record_kind_filter = retrieval_policy_string_filter(&retrieval_policy, "recordKinds");
    let memory_kind_filter = retrieval_policy_string_filter(&retrieval_policy, "memoryKinds");
    let records = list_project_records(repo_root, project_id)?;
    let approved_memory =
        list_approved_agent_memories(repo_root, project_id, effective_agent_session_id.as_deref())?;
    let handoffs = list_agent_handoff_lineage_by_status(
        repo_root,
        project_id,
        &[
            AgentHandoffLineageStatus::Completed,
            AgentHandoffLineageStatus::TargetCreated,
            AgentHandoffLineageStatus::Recorded,
        ],
    )?;

    let project_records = records
        .iter()
        .filter(|record| knowledge_project_record_visible(record))
        .filter(|record| knowledge_record_kind_allowed(record, &record_kind_filter))
        .filter(|record| !is_current_problem_continuity_record(record))
        .take(limit)
        .map(knowledge_project_record_json)
        .collect::<Vec<_>>();
    let continuity_records = records
        .iter()
        .filter(|record| knowledge_project_record_visible(record))
        .filter(|record| knowledge_record_kind_allowed(record, &record_kind_filter))
        .filter(|record| is_current_problem_continuity_record(record))
        .take(limit)
        .map(knowledge_project_record_json)
        .collect::<Vec<_>>();
    let memories = approved_memory
        .iter()
        .filter(|memory| knowledge_memory_visible(memory))
        .filter(|memory| knowledge_memory_kind_allowed(memory, &memory_kind_filter))
        .take(limit)
        .map(knowledge_memory_json)
        .collect::<Vec<_>>();
    let handoff_records = handoffs
        .iter()
        .filter(|lineage| {
            knowledge_handoff_relevant(lineage, effective_agent_session_id.as_deref(), run_id)
        })
        .take(limit)
        .map(knowledge_handoff_json)
        .collect::<Vec<_>>();

    Ok(json!({
        "schema": "xero.agent_knowledge_inspection.v1",
        "projectId": project_id,
        "agentSessionId": effective_agent_session_id,
        "runId": run_id,
        "limit": limit,
        "retrievalPolicy": {
            "source": if runtime_audit.is_some() {
                "runtime_audit_export"
            } else {
                "not_requested"
            },
            "policy": retrieval_policy,
            "recordKindFilter": sorted_filter_values(&record_kind_filter),
            "memoryKindFilter": sorted_filter_values(&memory_kind_filter),
            "filtersApplied": !record_kind_filter.is_empty() || !memory_kind_filter.is_empty(),
        },
        "projectRecords": project_records,
        "continuityRecords": continuity_records,
        "approvedMemory": memories,
        "handoffRecords": handoff_records,
        "redaction": {
            "rawBlockedRecordsExcluded": true,
            "redactedProjectRecordTextHidden": true,
            "handoffBundleRawPayloadHidden": true,
        },
    }))
}

pub fn load_agent_handoff_context_summary(
    repo_root: &Path,
    project_id: &str,
    handoff_id: &str,
) -> Result<JsonValue, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_handoff_context_summary_project_required",
    )?;
    validate_non_empty_text(
        handoff_id,
        "handoffId",
        "agent_handoff_context_summary_handoff_required",
    )?;
    let lineage = get_agent_handoff_lineage_by_handoff_id(repo_root, project_id, handoff_id)?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_handoff_context_summary_not_found",
                format!("Xero could not find handoff `{handoff_id}` for project `{project_id}`."),
            )
        })?;
    Ok(handoff_context_summary_json(&lineage))
}

fn knowledge_project_record_visible(record: &ProjectRecordRecord) -> bool {
    record.visibility == ProjectRecordVisibility::Retrieval
        && record.redaction_state != ProjectRecordRedactionState::Blocked
        && record.freshness_state == "current"
        && record.superseded_by_id.is_none()
}

fn knowledge_record_kind_allowed(
    record: &ProjectRecordRecord,
    allowed_kinds: &HashSet<String>,
) -> bool {
    allowed_kinds.is_empty()
        || allowed_kinds.contains(project_record_kind_sql_value(&record.record_kind))
}

fn is_current_problem_continuity_record(record: &ProjectRecordRecord) -> bool {
    record.schema_name.as_deref() == Some("xero.project_record.current_problem_continuity.v1")
}

fn knowledge_project_record_json(record: &ProjectRecordRecord) -> JsonValue {
    json!({
        "recordId": record.record_id,
        "recordKind": project_record_kind_sql_value(&record.record_kind),
        "title": record.title,
        "summary": redaction_safe_project_record_text(record, &record.summary),
        "textPreview": redaction_safe_project_record_text(record, &record.text),
        "schemaName": record.schema_name,
        "importance": project_record_importance_label(&record.importance),
        "confidence": record.confidence,
        "tags": record.tags,
        "relatedPaths": record.related_paths,
        "freshnessState": record.freshness_state,
        "redactionState": project_record_redaction_label(&record.redaction_state),
        "sourceItemIds": record.source_item_ids,
        "updatedAt": record.updated_at,
    })
}

fn redaction_safe_project_record_text(record: &ProjectRecordRecord, value: &str) -> JsonValue {
    if record.redaction_state == ProjectRecordRedactionState::Clean {
        json!(text_preview(value))
    } else {
        JsonValue::Null
    }
}

fn knowledge_memory_visible(memory: &AgentMemoryRecord) -> bool {
    memory.enabled
        && memory.review_state == AgentMemoryReviewState::Approved
        && !matches!(
            memory.freshness_state.as_str(),
            "stale" | "superseded" | "blocked"
        )
        && memory.superseded_by_id.is_none()
}

fn knowledge_memory_kind_allowed(
    memory: &AgentMemoryRecord,
    allowed_kinds: &HashSet<String>,
) -> bool {
    allowed_kinds.is_empty() || allowed_kinds.contains(agent_memory_kind_label(&memory.kind))
}

fn agent_memory_kind_label(kind: &AgentMemoryKind) -> &'static str {
    match kind {
        AgentMemoryKind::ProjectFact => "project_fact",
        AgentMemoryKind::UserPreference => "user_preference",
        AgentMemoryKind::Decision => "decision",
        AgentMemoryKind::SessionSummary => "session_summary",
        AgentMemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn knowledge_memory_json(memory: &AgentMemoryRecord) -> JsonValue {
    json!({
        "memoryId": memory.memory_id,
        "scope": memory.scope,
        "kind": memory.kind,
        "textPreview": text_preview(&memory.text),
        "confidence": memory.confidence,
        "sourceRunId": memory.source_run_id,
        "sourceItemIds": memory.source_item_ids,
        "freshnessState": memory.freshness_state,
        "updatedAt": memory.updated_at,
    })
}

fn retrieval_policy_string_filter(policy: &JsonValue, field: &str) -> HashSet<String> {
    policy
        .get(field)
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
        .filter_map(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn sorted_filter_values(values: &HashSet<String>) -> Vec<String> {
    let mut values = values.iter().cloned().collect::<Vec<_>>();
    values.sort();
    values
}

fn knowledge_handoff_relevant(
    lineage: &AgentHandoffLineageRecord,
    agent_session_id: Option<&str>,
    run_id: Option<&str>,
) -> bool {
    if agent_session_id.is_none() && run_id.is_none() {
        return true;
    }
    if run_id.is_some_and(|run_id| {
        lineage.source_run_id == run_id || lineage.target_run_id.as_deref() == Some(run_id)
    }) {
        return true;
    }
    agent_session_id.is_some_and(|agent_session_id| {
        lineage.source_agent_session_id == agent_session_id
            || lineage.target_agent_session_id.as_deref() == Some(agent_session_id)
    })
}

fn knowledge_handoff_json(lineage: &AgentHandoffLineageRecord) -> JsonValue {
    json!({
        "handoffId": lineage.handoff_id,
        "status": handoff_lineage_status_label(&lineage.status),
        "sourceRunId": lineage.source_run_id,
        "targetRunId": lineage.target_run_id,
        "runtimeAgentId": lineage.source_runtime_agent_id.as_str(),
        "agentDefinitionId": lineage.source_agent_definition_id,
        "agentDefinitionVersion": lineage.source_agent_definition_version,
        "providerId": lineage.provider_id,
        "modelId": lineage.model_id,
        "handoffRecordId": lineage.handoff_record_id,
        "bundleKeys": lineage
            .bundle
            .as_object()
            .map(|object| object.keys().cloned().collect::<Vec<_>>())
            .unwrap_or_default(),
        "createdAt": lineage.created_at,
        "updatedAt": lineage.updated_at,
    })
}

fn text_preview(value: &str) -> String {
    value.chars().take(240).collect()
}

fn project_record_importance_label(importance: &ProjectRecordImportance) -> &'static str {
    match importance {
        ProjectRecordImportance::Low => "low",
        ProjectRecordImportance::Normal => "normal",
        ProjectRecordImportance::High => "high",
        ProjectRecordImportance::Critical => "critical",
    }
}

fn project_record_redaction_label(redaction: &ProjectRecordRedactionState) -> &'static str {
    match redaction {
        ProjectRecordRedactionState::Clean => "clean",
        ProjectRecordRedactionState::Redacted => "redacted",
        ProjectRecordRedactionState::Blocked => "blocked",
    }
}

fn handoff_lineage_status_label(status: &AgentHandoffLineageStatus) -> &'static str {
    match status {
        AgentHandoffLineageStatus::Pending => "pending",
        AgentHandoffLineageStatus::Recorded => "recorded",
        AgentHandoffLineageStatus::TargetCreated => "target_created",
        AgentHandoffLineageStatus::Completed => "completed",
        AgentHandoffLineageStatus::Failed => "failed",
    }
}

fn handoff_context_summary_json(lineage: &AgentHandoffLineageRecord) -> JsonValue {
    let (safe_bundle, summary_redacted) =
        crate::runtime::redaction::redact_json_for_persistence(&lineage.bundle);
    let bundle_redaction_state = safe_bundle
        .get("redactionState")
        .and_then(JsonValue::as_str)
        .unwrap_or(if summary_redacted {
            "redacted"
        } else {
            "unknown"
        });
    json!({
        "schema": "xero.agent_handoff_context_summary.v1",
        "projectId": lineage.project_id,
        "handoffId": lineage.handoff_id,
        "status": handoff_lineage_status_label(&lineage.status),
        "source": {
            "agentSessionId": lineage.source_agent_session_id,
            "runId": lineage.source_run_id,
            "runtimeAgentId": lineage.source_runtime_agent_id.as_str(),
            "agentDefinitionId": lineage.source_agent_definition_id,
            "agentDefinitionVersion": lineage.source_agent_definition_version,
            "contextHash": lineage.source_context_hash,
        },
        "target": {
            "agentSessionId": lineage.target_agent_session_id,
            "runId": lineage.target_run_id,
            "runtimeAgentId": lineage.target_runtime_agent_id.as_str(),
            "agentDefinitionId": lineage.target_agent_definition_id,
            "agentDefinitionVersion": lineage.target_agent_definition_version,
        },
        "provider": {
            "providerId": lineage.provider_id,
            "modelId": lineage.model_id,
        },
        "carriedContext": {
            "userGoal": handoff_bundle_field(&safe_bundle, "userGoal"),
            "currentTask": handoff_bundle_field(&safe_bundle, "currentTask"),
            "currentStatus": handoff_bundle_field(&safe_bundle, "currentStatus"),
            "completedWork": handoff_bundle_array(&safe_bundle, "completedWork"),
            "pendingWork": handoff_bundle_array(&safe_bundle, "pendingWork"),
            "activeTodoItems": handoff_bundle_array(&safe_bundle, "activeTodoItems"),
            "importantDecisions": handoff_bundle_array(&safe_bundle, "importantDecisions"),
            "constraints": handoff_bundle_array(&safe_bundle, "constraints"),
            "durableContext": handoff_bundle_field(&safe_bundle, "durableContext"),
            "workingSetSummary": handoff_bundle_field(&safe_bundle, "workingSetSummary"),
            "sourceCitedContinuityRecords": handoff_bundle_array(&safe_bundle, "sourceCitedContinuityRecords"),
            "recentFileChanges": handoff_bundle_array(&safe_bundle, "recentFileChanges"),
            "toolAndCommandEvidence": handoff_bundle_array(&safe_bundle, "toolAndCommandEvidence"),
            "verificationStatus": handoff_bundle_field(&safe_bundle, "verificationStatus"),
            "knownRisks": handoff_bundle_array(&safe_bundle, "knownRisks"),
            "openQuestions": handoff_bundle_array(&safe_bundle, "openQuestions"),
            "approvedMemories": handoff_bundle_array(&safe_bundle, "approvedMemories"),
            "relevantProjectRecords": handoff_bundle_array(&safe_bundle, "relevantProjectRecords"),
            "agentSpecific": handoff_bundle_field(&safe_bundle, "agentSpecific"),
        },
        "omittedContext": [
            {
                "kind": "raw_system_messages",
                "status": "omitted",
                "reason": "System and developer instructions are not copied as handoff context; the target run receives current higher-priority policy separately."
            },
            {
                "kind": "raw_transcript",
                "status": "summarized",
                "reason": "Only bounded summaries, recent message references, and source-cited continuity records are carried forward.",
                "referenceCount": handoff_bundle_array(&safe_bundle, "recentRawTailMessageReferences").as_array().map(Vec::len).unwrap_or(0)
            },
            {
                "kind": "raw_durable_context",
                "status": "tool_mediated",
                "reason": "Durable project context is referenced through the project_context tool instead of injected as raw prompt text."
            },
            {
                "kind": "raw_bundle_payload",
                "status": "hidden",
                "reason": "This summary exposes only whitelisted carried-context fields and redaction metadata."
            }
        ],
        "redaction": {
            "state": if summary_redacted { "redacted" } else { bundle_redaction_state },
            "bundleRedactionCount": safe_bundle.get("redactionCount").cloned().unwrap_or(JsonValue::Null),
            "summaryRedactionApplied": summary_redacted,
            "rawPayloadHidden": true,
        },
        "safetyRationale": {
            "sameRuntimeAgent": lineage.source_runtime_agent_id == lineage.target_runtime_agent_id,
            "sameDefinitionVersion": lineage.source_agent_definition_id == lineage.target_agent_definition_id
                && lineage.source_agent_definition_version == lineage.target_agent_definition_version,
            "sourceContextHashPresent": !lineage.source_context_hash.trim().is_empty(),
            "targetRunCreated": lineage.target_run_id.is_some(),
            "handoffRecordPersisted": lineage.handoff_record_id.is_some(),
            "reasons": [
                "The target continues under the current system, repository, approval, and tool policy.",
                "Stored handoff content is source-cited data, not higher-priority instruction.",
                "Raw durable context remains tool-mediated and can be refreshed through project_context.",
                "The handoff lineage pins runtime, provider/model, agent definition, source context hash, target run, and status."
            ]
        },
        "createdAt": lineage.created_at,
        "updatedAt": lineage.updated_at,
        "completedAt": lineage.completed_at,
        "uiDeferred": true,
    })
}

fn handoff_bundle_field(bundle: &JsonValue, field: &str) -> JsonValue {
    bundle.get(field).cloned().unwrap_or(JsonValue::Null)
}

fn handoff_bundle_array(bundle: &JsonValue, field: &str) -> JsonValue {
    bundle
        .get(field)
        .and_then(JsonValue::as_array)
        .cloned()
        .map(JsonValue::Array)
        .unwrap_or_else(|| json!([]))
}

fn merge_audit_events(
    primary: Vec<AgentRuntimeAuditEventRecord>,
    secondary: Vec<AgentRuntimeAuditEventRecord>,
) -> Vec<AgentRuntimeAuditEventRecord> {
    let mut seen = HashSet::new();
    let mut merged = Vec::with_capacity(primary.len() + secondary.len());
    for event in primary.into_iter().chain(secondary) {
        if seen.insert(event.audit_id.clone()) {
            merged.push(event);
        }
    }
    merged.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    merged
}

fn audit_export_permission_explanations(
    agent_definition_id: &str,
    audit_events: &[AgentRuntimeAuditEventRecord],
) -> Vec<JsonValue> {
    let mut explanations = Vec::new();
    let mut seen = HashSet::new();
    push_permission_explanation(
        &mut explanations,
        &mut seen,
        capability_permission_explanation("custom_agent", agent_definition_id),
    );
    for event in audit_events {
        if let Some(permission) = event.payload.get("permission") {
            push_permission_explanation(&mut explanations, &mut seen, permission.clone());
        }
    }
    explanations
}

fn push_permission_explanation(
    explanations: &mut Vec<JsonValue>,
    seen: &mut HashSet<String>,
    explanation: JsonValue,
) {
    if explanation.get("schema").and_then(JsonValue::as_str)
        != Some("xero.capability_permission_explanation.v1")
    {
        return;
    }
    let key = explanation
        .get("subjectKind")
        .and_then(JsonValue::as_str)
        .zip(explanation.get("subjectId").and_then(JsonValue::as_str))
        .map(|(kind, id)| format!("{kind}\0{id}"))
        .unwrap_or_else(|| explanation.to_string());
    if seen.insert(key) {
        explanations.push(explanation);
    }
}

fn block_custom_agent_definition(
    connection: &Connection,
    record: &NewAgentCapabilityRevocationRecord,
) -> Result<(), CommandError> {
    let scope: Option<String> = connection
        .query_row(
            "SELECT scope FROM agent_definitions WHERE definition_id = ?1",
            params![record.subject_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(map_audit_read_error)?;
    let scope = scope.ok_or_else(|| {
        CommandError::user_fixable(
            "agent_definition_not_found",
            format!(
                "Xero could not revoke custom agent `{}` because it was not found.",
                record.subject_id
            ),
        )
    })?;
    if scope == "built_in" {
        return Err(CommandError::user_fixable(
            "agent_definition_builtin_immutable",
            format!(
                "Xero cannot emergency-disable built-in agent definition `{}` through custom-agent revocation.",
                record.subject_id
            ),
        ));
    }
    connection
        .execute(
            r#"
            UPDATE agent_definitions
            SET lifecycle_state = 'blocked',
                updated_at = ?2
            WHERE definition_id = ?1
            "#,
            params![record.subject_id, record.created_at],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_definition_emergency_disable_failed",
                format!(
                    "Xero could not emergency-disable custom agent `{}`: {error}",
                    record.subject_id
                ),
            )
        })?;
    Ok(())
}

fn insert_audit_event(
    connection: &Connection,
    record: &NewAgentRuntimeAuditEventRecord,
) -> Result<(), CommandError> {
    validate_new_audit_event(record)?;
    let payload_json = serde_json::to_string(&record.payload).map_err(|error| {
        CommandError::system_fault(
            "agent_runtime_audit_payload_encode_failed",
            format!("Xero could not encode agent runtime audit payload: {error}"),
        )
    })?;
    connection
        .execute(
            r#"
            INSERT INTO agent_runtime_audit_events (
                audit_id,
                project_id,
                actor_kind,
                actor_id,
                action_kind,
                subject_kind,
                subject_id,
                run_id,
                agent_definition_id,
                agent_definition_version,
                risk_class,
                approval_action_id,
                payload_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(project_id, audit_id) DO NOTHING
            "#,
            params![
                record.audit_id,
                record.project_id,
                record.actor_kind,
                record.actor_id,
                record.action_kind,
                record.subject_kind,
                record.subject_id,
                record.run_id,
                record.agent_definition_id,
                record.agent_definition_version.map(i64::from),
                record.risk_class,
                record.approval_action_id,
                payload_json,
                record.created_at,
            ],
        )
        .map(|_| ())
        .map_err(|error| {
            CommandError::retryable(
                "agent_runtime_audit_event_insert_failed",
                format!("Xero could not persist agent runtime audit event: {error}"),
            )
        })
}

fn read_revocation_by_id(
    connection: &Connection,
    project_id: &str,
    revocation_id: &str,
) -> Result<AgentCapabilityRevocationRecord, CommandError> {
    connection
        .query_row(
            revocation_select_sql("WHERE project_id = ?1 AND revocation_id = ?2").as_str(),
            params![project_id, revocation_id],
            read_revocation_row,
        )
        .optional()
        .map_err(map_audit_read_error)?
        .transpose()?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "agent_capability_revocation_not_found",
                format!("Xero could not find capability revocation `{revocation_id}`."),
            )
        })
}

fn audit_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            audit_id,
            project_id,
            actor_kind,
            actor_id,
            action_kind,
            subject_kind,
            subject_id,
            run_id,
            agent_definition_id,
            agent_definition_version,
            risk_class,
            approval_action_id,
            payload_json,
            created_at
        FROM agent_runtime_audit_events
        {where_clause}
        "#
    )
}

fn revocation_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            revocation_id,
            project_id,
            subject_kind,
            subject_id,
            scope_json,
            reason,
            created_by,
            status,
            created_at,
            cleared_at
        FROM agent_capability_revocations
        {where_clause}
        "#
    )
}

fn read_audit_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentRuntimeAuditEventRecord, CommandError>> {
    let payload_json: String = row.get(13)?;
    let version: Option<i64> = row.get(10)?;
    Ok(Ok(AgentRuntimeAuditEventRecord {
        id: row.get(0)?,
        audit_id: row.get(1)?,
        project_id: row.get(2)?,
        actor_kind: row.get(3)?,
        actor_id: row.get(4)?,
        action_kind: row.get(5)?,
        subject_kind: row.get(6)?,
        subject_id: row.get(7)?,
        run_id: row.get(8)?,
        agent_definition_id: row.get(9)?,
        agent_definition_version: version.and_then(|value| u32::try_from(value).ok()),
        risk_class: row.get(11)?,
        approval_action_id: row.get(12)?,
        payload: serde_json::from_str(&payload_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                13,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        created_at: row.get(14)?,
    }))
}

fn read_revocation_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentCapabilityRevocationRecord, CommandError>> {
    let scope_json: String = row.get(5)?;
    Ok(Ok(AgentCapabilityRevocationRecord {
        id: row.get(0)?,
        revocation_id: row.get(1)?,
        project_id: row.get(2)?,
        subject_kind: row.get(3)?,
        subject_id: row.get(4)?,
        scope: serde_json::from_str(&scope_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        reason: row.get(6)?,
        created_by: row.get(7)?,
        status: row.get(8)?,
        created_at: row.get(9)?,
        cleared_at: row.get(10)?,
    }))
}

fn validate_new_audit_event(record: &NewAgentRuntimeAuditEventRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.audit_id,
        "auditId",
        "agent_runtime_audit_id_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_runtime_audit_project_required",
    )?;
    validate_actor_kind(&record.actor_kind)?;
    validate_non_empty_text(
        &record.action_kind,
        "actionKind",
        "agent_runtime_audit_action_required",
    )?;
    validate_subject_kind(&record.subject_kind)?;
    validate_non_empty_text(
        &record.subject_id,
        "subjectId",
        "agent_runtime_audit_subject_required",
    )?;
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_runtime_audit_created_at_required",
    )?;
    Ok(())
}

fn validate_new_revocation(
    record: &NewAgentCapabilityRevocationRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.revocation_id,
        "revocationId",
        "agent_capability_revocation_id_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_capability_revocation_project_required",
    )?;
    validate_subject_kind(&record.subject_kind)?;
    validate_non_empty_text(
        &record.subject_id,
        "subjectId",
        "agent_capability_revocation_subject_required",
    )?;
    validate_non_empty_text(
        &record.reason,
        "reason",
        "agent_capability_revocation_reason_required",
    )?;
    validate_non_empty_text(
        &record.created_by,
        "createdBy",
        "agent_capability_revocation_created_by_required",
    )?;
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_capability_revocation_created_at_required",
    )?;
    Ok(())
}

fn validate_actor_kind(kind: &str) -> Result<(), CommandError> {
    if matches!(kind, "system" | "user" | "agent" | "runtime") {
        Ok(())
    } else {
        Err(CommandError::invalid_request("actorKind"))
    }
}

fn validate_subject_kind(kind: &str) -> Result<(), CommandError> {
    if matches!(
        kind,
        "custom_agent"
            | "tool_pack"
            | "external_integration"
            | "browser_control"
            | "destructive_write"
    ) {
        Ok(())
    } else {
        Err(CommandError::invalid_request("subjectKind"))
    }
}

fn short_stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    let hash = format!("{:x}", hasher.finalize());
    format!("{prefix}-{}", &hash[..16])
}

fn map_audit_read_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "agent_runtime_audit_read_failed",
        format!("Xero could not read agent audit state: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::{fs, path::Path};

    use rusqlite::{params, Connection};
    use serde_json::json;

    use crate::{
        commands::RuntimeAgentIdDto,
        db::{
            configure_connection,
            migrations::migrations,
            project_store::{
                archive_agent_definition, create_agent_session, insert_agent_context_manifest,
                insert_agent_definition, insert_agent_handoff_lineage, insert_agent_memory,
                insert_agent_run, insert_project_record, load_agent_definition,
                resolve_agent_definition_for_run, AgentContextBudgetPressure,
                AgentContextManifestRequestKind, AgentContextPolicyAction,
                AgentContextRedactionState, AgentHandoffLineageStatus, AgentMemoryKind,
                AgentMemoryReviewState, AgentMemoryScope, AgentSessionCreateRecord,
                NewAgentContextManifestRecord, NewAgentDefinitionRecord,
                NewAgentHandoffLineageRecord, NewAgentMemoryRecord, NewAgentRunRecord,
                NewProjectRecordRecord, ProjectRecordImportance, ProjectRecordKind,
                ProjectRecordRedactionState, ProjectRecordVisibility,
                BUILTIN_AGENT_DEFINITION_VERSION,
            },
            register_project_database_path,
        },
    };

    fn create_project_database(repo_root: &Path, project_id: &str) -> std::path::PathBuf {
        let database_path = repo_root
            .parent()
            .expect("repo parent")
            .join("app-data")
            .join("projects")
            .join(project_id)
            .join("state.db");
        fs::create_dir_all(database_path.parent().expect("database parent")).expect("database dir");
        let mut connection = Connection::open(&database_path).expect("open database");
        configure_connection(&connection).expect("configure database");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate database");
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
        register_project_database_path(repo_root, &database_path);
        database_path
    }

    fn custom_definition(definition_id: &str) -> NewAgentDefinitionRecord {
        custom_definition_version(definition_id, 1, "2026-05-09T00:00:00Z")
    }

    fn custom_definition_version(
        definition_id: &str,
        version: u32,
        now: &str,
    ) -> NewAgentDefinitionRecord {
        NewAgentDefinitionRecord {
            definition_id: definition_id.into(),
            version,
            display_name: "Project Researcher".into(),
            short_label: "Research".into(),
            description: "Research project state.".into(),
            scope: "project_custom".into(),
            lifecycle_state: "active".into(),
            base_capability_profile: "observe_only".into(),
            snapshot: json!({
                "schema": "xero.agent_definition.v1",
                "schemaVersion": 1,
                "id": definition_id,
                "version": version,
                "displayName": "Project Researcher",
                "shortLabel": "Research",
                "description": "Research project state.",
                "taskPurpose": "Answer project questions.",
                "scope": "project_custom",
                "lifecycleState": "active",
                "baseCapabilityProfile": "observe_only",
                "defaultApprovalMode": "suggest",
                "allowedApprovalModes": ["suggest"],
                "promptPolicy": "ask",
                "toolPolicy": {
                    "allowedEffectClasses": ["observe"],
                    "allowedTools": ["project_context_search"],
                    "deniedTools": [],
                    "allowedToolGroups": ["project_context"],
                    "deniedToolGroups": [],
                    "externalServiceAllowed": false,
                    "browserControlAllowed": false,
                    "skillRuntimeAllowed": false,
                    "subagentAllowed": false,
                    "commandAllowed": false,
                    "destructiveWriteAllowed": false
                },
                "workflowContract": "Answer project questions from reviewed project state.",
                "finalResponseContract": "Return a concise answer with uncertainty called out.",
                "prompts": [
                    {
                        "id": "project-researcher-intent",
                        "label": "Project Researcher Intent",
                        "role": "developer",
                        "source": "test",
                        "body": "Answer project questions using only reviewed project state."
                    }
                ],
                "tools": [],
                "output": {
                    "contract": "answer",
                    "label": "Answer",
                    "description": "Answer the project question.",
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
                "outputContract": "answer",
                "dbTouchpoints": {
                    "reads": [
                        {
                            "table": "project_records",
                            "kind": "read",
                            "purpose": "Read reviewed project state.",
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
                "handoffPolicy": { "enabled": true, "preserveDefinitionVersion": true },
                "memoryPolicy": { "reviewRequired": true },
                "retrievalDefaults": {
                    "enabled": true,
                    "limit": 4,
                    "recordKinds": ["artifact", "context_note"],
                    "memoryKinds": ["project_fact"]
                }
            }),
            validation_report: Some(json!({ "status": "valid", "diagnostics": [] })),
            created_at: now.into(),
            updated_at: now.into(),
        }
    }

    fn knowledge_record(
        project_id: &str,
        record_id: &str,
        kind: ProjectRecordKind,
        schema_name: Option<&str>,
        text: &str,
        redaction_state: ProjectRecordRedactionState,
    ) -> NewProjectRecordRecord {
        NewProjectRecordRecord {
            record_id: record_id.into(),
            project_id: project_id.into(),
            record_kind: kind,
            runtime_agent_id: RuntimeAgentIdDto::Ask,
            agent_definition_id: "ask".into(),
            agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
            agent_session_id: None,
            run_id: "run-knowledge-source".into(),
            workflow_run_id: None,
            workflow_step_id: None,
            title: format!("Knowledge {record_id}"),
            summary: text.into(),
            text: text.into(),
            content_json: schema_name.map(|schema| json!({ "schema": schema, "text": text })),
            schema_name: schema_name.map(str::to_owned),
            schema_version: 1,
            importance: ProjectRecordImportance::High,
            confidence: Some(0.9),
            tags: vec!["knowledge".into()],
            source_item_ids: vec![format!("source:{record_id}")],
            related_paths: vec!["src/lib.rs".into()],
            produced_artifact_refs: Vec::new(),
            redaction_state,
            visibility: ProjectRecordVisibility::Retrieval,
            created_at: "2026-05-09T00:10:00Z".into(),
        }
    }

    #[test]
    fn s15_database_touchpoint_explanation_reports_saved_definition_contract() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        let project_id = "project-db-touchpoint-explanation";
        create_project_database(&repo_root, project_id);
        let definition_id = "project_researcher";
        let mut definition = custom_definition(definition_id);
        definition.snapshot["dbTouchpoints"]["writes"] = json!([
            {
                "table": "agent_runtime_audit_events",
                "kind": "write",
                "purpose": "Record compact touchpoint audit metadata.",
                "triggers": [{ "kind": "tool", "name": "project_context_search" }],
                "columns": ["payload_json"]
            }
        ]);
        insert_agent_definition(&repo_root, &definition).expect("insert custom definition");

        let explanation =
            load_agent_database_touchpoint_explanation(&repo_root, project_id, definition_id, 1)
                .expect("load touchpoint explanation");

        assert_eq!(
            explanation["schema"],
            json!("xero.agent_database_touchpoint_explanation.v1")
        );
        assert_eq!(explanation["summary"]["readCount"], json!(1));
        assert_eq!(explanation["summary"]["writeCount"], json!(1));
        assert_eq!(explanation["summary"]["hasWrites"], json!(true));
        assert_eq!(
            explanation["touchpoints"]["writes"][0]["table"],
            json!("agent_runtime_audit_events")
        );
        assert_eq!(
            explanation["touchpoints"]["writes"][0]["triggerCount"],
            json!(1)
        );
        assert_eq!(explanation["source"]["path"], json!("dbTouchpoints"));
        assert_eq!(explanation["uiDeferred"], json!(true));
    }

    #[test]
    fn s52_tool_pack_permission_explanation_projects_pack_policy_boundaries() {
        let explanation = capability_permission_explanation("tool_pack", "project_context");

        assert_eq!(
            explanation["schema"],
            json!("xero.capability_permission_explanation.v1")
        );
        assert_eq!(explanation["subjectKind"], json!("tool_pack"));
        assert_eq!(explanation["toolPack"]["packId"], json!("project_context"));
        assert_eq!(
            explanation["networkAccess"],
            json!("can_leave_local_machine")
        );
        assert_eq!(
            explanation["fileMutation"],
            json!("can_mutate_project_state")
        );
        assert!(explanation["toolPack"]["allowedEffectClasses"]
            .as_array()
            .expect("allowed effects")
            .contains(&json!("runtime_state")));
        assert!(explanation["toolPack"]["deniedEffectClasses"]
            .as_array()
            .expect("denied effects")
            .contains(&json!("destructive_write")));
        assert!(explanation["toolPack"]["reviewRequirements"]
            .as_array()
            .expect("review requirements")
            .iter()
            .any(|requirement| requirement["requirementId"] == json!("durable_context_review")));
    }

    #[test]
    fn s53_custom_agent_revocation_blocks_new_runs_without_deleting_audit_history() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        let project_id = "project-agent-revocation";
        create_project_database(&repo_root, project_id);
        let definition_id = "project_researcher";
        insert_agent_definition(&repo_root, &custom_definition(definition_id))
            .expect("insert custom definition");

        let revocation = revoke_agent_capability(
            &repo_root,
            &NewAgentCapabilityRevocationRecord {
                revocation_id: agent_capability_revocation_id(
                    project_id,
                    "custom_agent",
                    definition_id,
                ),
                project_id: project_id.into(),
                subject_kind: "custom_agent".into(),
                subject_id: definition_id.into(),
                scope: json!({ "reason": "test emergency disable" }),
                reason: "Emergency disable during audit test.".into(),
                created_by: "test-user".into(),
                created_at: "2026-05-09T00:01:00Z".into(),
            },
        )
        .expect("revoke custom agent");

        assert_eq!(revocation.status, "active");
        let definition = load_agent_definition(&repo_root, definition_id)
            .expect("load definition")
            .expect("definition exists");
        assert_eq!(definition.lifecycle_state, "blocked");
        let error = resolve_agent_definition_for_run(
            &repo_root,
            Some(definition_id),
            RuntimeAgentIdDto::Ask,
        )
        .expect_err("blocked custom agent cannot start");
        assert_eq!(error.code, "agent_definition_inactive");
        let audit_events = list_agent_runtime_audit_events_for_subject(
            &repo_root,
            project_id,
            "custom_agent",
            definition_id,
        )
        .expect("list audit events");
        assert!(audit_events
            .iter()
            .any(|event| event.action_kind == "capability_revoked"));
        assert!(audit_events.iter().any(|event| {
            event.payload["permission"]["schema"]
                == json!("xero.capability_permission_explanation.v1")
        }));
    }

    #[test]
    fn s56_runtime_audit_export_includes_manifest_and_policy_context() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        let project_id = "project-runtime-audit-export";
        create_project_database(&repo_root, project_id);
        let definition_id = "project_researcher";
        insert_agent_definition(&repo_root, &custom_definition(definition_id))
            .expect("insert custom definition");
        let session = create_agent_session(
            &repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Audit export".into(),
                summary: String::new(),
                selected: true,
            },
        )
        .expect("create session");
        insert_agent_run(
            &repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: Some(BUILTIN_AGENT_DEFINITION_VERSION),
                project_id: project_id.into(),
                agent_session_id: session.agent_session_id.clone(),
                run_id: "run-audit-export".into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Export the audit context.".into(),
                system_prompt: "system".into(),
                now: "2026-05-09T00:02:00Z".into(),
            },
        )
        .expect("insert run");
        insert_agent_context_manifest(
            &repo_root,
            &NewAgentContextManifestRecord {
                manifest_id: "manifest-audit-export".into(),
                project_id: project_id.into(),
                agent_session_id: session.agent_session_id,
                run_id: Some("run-audit-export".into()),
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: definition_id.into(),
                agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                provider_id: Some("test-provider".into()),
                model_id: Some("test-model".into()),
                request_kind: AgentContextManifestRequestKind::ProviderTurn,
                policy_action: AgentContextPolicyAction::ContinueNow,
                policy_reason_code: "within_budget".into(),
                budget_tokens: Some(1000),
                estimated_tokens: 100,
                pressure: AgentContextBudgetPressure::Low,
                context_hash: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    .into(),
                included_contributors: Vec::new(),
                excluded_contributors: Vec::new(),
                retrieval_query_ids: Vec::new(),
                retrieval_result_ids: Vec::new(),
                compaction_id: None,
                handoff_id: None,
                redaction_state: AgentContextRedactionState::Clean,
                manifest: json!({
                    "promptFragments": [{ "id": "runtime.base", "title": "Base" }],
                    "toolDescriptors": [{ "name": "project_context_search" }],
                    "toolExposurePlan": { "active": ["project_context_search"] },
                    "retrieval": { "deliveryModel": "tool_mediated", "resultCount": 0 }
                }),
                created_at: "2026-05-09T00:03:00Z".into(),
            },
        )
        .expect("insert context manifest");
        record_agent_runtime_audit_event(
            &repo_root,
            &NewAgentRuntimeAuditEventRecord {
                audit_id: agent_runtime_audit_id(
                    project_id,
                    "capability_approval_granted",
                    "tool_pack",
                    "project_context_tools",
                    "2026-05-09T00:03:30Z",
                ),
                project_id: project_id.into(),
                actor_kind: "user".into(),
                actor_id: Some("test-user".into()),
                action_kind: "capability_approval_granted".into(),
                subject_kind: "tool_pack".into(),
                subject_id: "project_context_tools".into(),
                run_id: Some("run-audit-export".into()),
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: Some(BUILTIN_AGENT_DEFINITION_VERSION),
                risk_class: Some("tool_pack_grant".into()),
                approval_action_id: Some("approval-project-context-tools".into()),
                payload: json!({
                    "permission": capability_permission_explanation("tool_pack", "project_context_tools"),
                    "reason": "Allow project context search for this run."
                }),
                created_at: "2026-05-09T00:03:30Z".into(),
            },
        )
        .expect("record run capability audit event");

        let export = export_agent_runtime_audit(&repo_root, project_id, "run-audit-export")
            .expect("export audit");
        assert_eq!(export.agent_definition_id, definition_id);
        assert_eq!(
            export.context_manifest_ids,
            vec!["manifest-audit-export".to_string()]
        );
        assert_eq!(
            export.tool_policy["toolDescriptors"][0]["name"],
            json!("project_context_search")
        );
        assert_eq!(export.memory_policy["reviewRequired"], json!(true));
        assert_eq!(export.handoff_policy["enabled"], json!(true));
        assert!(export.capability_permission_explanations.contains(
            &capability_permission_explanation("custom_agent", definition_id)
        ));
        assert!(export.capability_permission_explanations.contains(
            &capability_permission_explanation("tool_pack", "project_context_tools")
        ));
        assert!(export.risky_capability_approvals.iter().any(|event| {
            event.subject_kind == "tool_pack"
                && event.run_id.as_deref() == Some("run-audit-export")
                && event.approval_action_id.as_deref() == Some("approval-project-context-tools")
        }));
    }

    #[test]
    fn s66_run_start_explanation_matches_runtime_audit_and_manifest() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        let project_id = "project-run-start-explanation";
        create_project_database(&repo_root, project_id);
        let definition_id = "project_researcher";
        insert_agent_definition(&repo_root, &custom_definition(definition_id))
            .expect("insert custom definition");
        let session = create_agent_session(
            &repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Run start explanation".into(),
                summary: String::new(),
                selected: true,
            },
        )
        .expect("create session");
        insert_agent_run(
            &repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: Some(BUILTIN_AGENT_DEFINITION_VERSION),
                project_id: project_id.into(),
                agent_session_id: session.agent_session_id.clone(),
                run_id: "run-start-explanation".into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Explain what will run.".into(),
                system_prompt: "system".into(),
                now: "2026-05-09T00:02:00Z".into(),
            },
        )
        .expect("insert run");
        insert_agent_context_manifest(
            &repo_root,
            &NewAgentContextManifestRecord {
                manifest_id: "manifest-run-start-explanation".into(),
                project_id: project_id.into(),
                agent_session_id: session.agent_session_id,
                run_id: Some("run-start-explanation".into()),
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: definition_id.into(),
                agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                provider_id: Some("test-provider".into()),
                model_id: Some("test-model".into()),
                request_kind: AgentContextManifestRequestKind::ProviderTurn,
                policy_action: AgentContextPolicyAction::ContinueNow,
                policy_reason_code: "within_budget".into(),
                budget_tokens: Some(1000),
                estimated_tokens: 100,
                pressure: AgentContextBudgetPressure::Low,
                context_hash: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
                    .into(),
                included_contributors: Vec::new(),
                excluded_contributors: Vec::new(),
                retrieval_query_ids: Vec::new(),
                retrieval_result_ids: Vec::new(),
                compaction_id: None,
                handoff_id: None,
                redaction_state: AgentContextRedactionState::Clean,
                manifest: json!({
                    "policy": {
                        "action": "continue_now",
                        "reasonCode": "within_budget",
                        "pressure": "low"
                    },
                    "promptFragments": [{ "id": "runtime.base", "title": "Base" }],
                    "toolDescriptors": [{ "name": "project_context_search" }],
                    "toolExposurePlan": { "active": ["project_context_search"] },
                    "retrieval": { "deliveryModel": "tool_mediated", "resultCount": 0 },
                    "handoff": { "status": "not_required" }
                }),
                created_at: "2026-05-09T00:03:00Z".into(),
            },
        )
        .expect("insert context manifest");

        let explanation =
            load_agent_run_start_explanation(&repo_root, project_id, "run-start-explanation")
                .expect("load run-start explanation");
        assert_eq!(
            explanation["schema"],
            json!("xero.agent_run_start_explanation.v1")
        );
        assert_eq!(
            explanation["definition"]["definitionId"],
            json!(definition_id)
        );
        assert_eq!(explanation["definition"]["version"], json!(1));
        assert_eq!(explanation["model"]["providerId"], json!("test-provider"));
        assert_eq!(explanation["model"]["modelId"], json!("test-model"));
        assert_eq!(explanation["approval"]["defaultMode"], json!("suggest"));
        assert_eq!(
            explanation["contextPolicy"]["reasonCode"],
            json!("within_budget")
        );
        assert_eq!(
            explanation["toolPolicy"]["toolExposurePlan"]["active"][0],
            json!("project_context_search")
        );
        assert_eq!(explanation["memoryPolicy"]["reviewRequired"], json!(true));
        assert_eq!(
            explanation["retrievalPolicy"]["deliveryModel"],
            json!("tool_mediated")
        );
        assert_eq!(
            explanation["handoffPolicy"]["status"],
            json!("not_required")
        );
        assert_eq!(
            explanation["databaseTouchpointExplanation"]["schema"],
            json!("xero.agent_database_touchpoint_explanation.v1")
        );
        assert_eq!(
            explanation["databaseTouchpointExplanation"]["definition"]["definitionId"],
            json!(definition_id)
        );
        assert_eq!(
            explanation["databaseTouchpointExplanation"]["touchpoints"]["reads"][0]["table"],
            json!("project_records")
        );
        assert!(explanation["capabilityPermissionExplanations"]
            .as_array()
            .expect("permission explanations")
            .contains(&capability_permission_explanation(
                "custom_agent",
                definition_id
            )));
        assert_eq!(
            explanation["source"]["contextManifestIds"],
            json!(["manifest-run-start-explanation"])
        );
    }

    #[test]
    fn s46_handoff_context_summary_is_redaction_safe_and_explains_omissions() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        let project_id = "project-handoff-summary";
        create_project_database(&repo_root, project_id);
        let definition_id = "project_researcher";
        insert_agent_definition(&repo_root, &custom_definition(definition_id))
            .expect("insert custom definition");
        let source_session = create_agent_session(
            &repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Handoff source".into(),
                summary: String::new(),
                selected: true,
            },
        )
        .expect("create source session");
        let target_session = create_agent_session(
            &repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Handoff target".into(),
                summary: String::new(),
                selected: false,
            },
        )
        .expect("create target session");
        insert_agent_run(
            &repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: Some(BUILTIN_AGENT_DEFINITION_VERSION),
                project_id: project_id.into(),
                agent_session_id: source_session.agent_session_id.clone(),
                run_id: "run-handoff-summary-source".into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Carry this work forward.".into(),
                system_prompt: "system".into(),
                now: "2026-05-09T00:21:00Z".into(),
            },
        )
        .expect("insert source run");
        insert_agent_run(
            &repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: Some(BUILTIN_AGENT_DEFINITION_VERSION),
                project_id: project_id.into(),
                agent_session_id: target_session.agent_session_id.clone(),
                run_id: "run-handoff-summary-target".into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Continue after handoff.".into(),
                system_prompt: "system".into(),
                now: "2026-05-09T00:22:00Z".into(),
            },
        )
        .expect("insert target run");
        insert_agent_handoff_lineage(
            &repo_root,
            &NewAgentHandoffLineageRecord {
                handoff_id: "handoff-summary".into(),
                project_id: project_id.into(),
                source_agent_session_id: source_session.agent_session_id,
                source_run_id: "run-handoff-summary-source".into(),
                source_runtime_agent_id: RuntimeAgentIdDto::Ask,
                source_agent_definition_id: definition_id.into(),
                source_agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                target_agent_session_id: Some(target_session.agent_session_id),
                target_run_id: Some("run-handoff-summary-target".into()),
                target_runtime_agent_id: RuntimeAgentIdDto::Ask,
                target_agent_definition_id: definition_id.into(),
                target_agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                source_context_hash:
                    "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".into(),
                status: AgentHandoffLineageStatus::Completed,
                idempotency_key: "run-handoff-summary-source:context:ask".into(),
                handoff_record_id: Some("record-handoff-summary".into()),
                bundle: json!({
                    "schema": "xero.agent_handoff.bundle.v1",
                    "schemaVersion": 1,
                    "userGoal": "Carry this work forward.",
                    "currentTask": "Continue safely with api_key=sk-s46-handoff-secret-value.",
                    "currentStatus": "Running",
                    "completedWork": [
                        {
                            "messageId": 10,
                            "createdAt": "2026-05-09T00:20:00Z",
                            "summary": "Implemented the backend summary contract."
                        }
                    ],
                    "pendingWork": [
                        { "kind": "user_prompt", "text": "Verify and continue." }
                    ],
                    "activeTodoItems": [],
                    "importantDecisions": [
                        { "summary": "Keep handoff context source-cited." }
                    ],
                    "constraints": [
                        "Follow current system, repository, approval, and tool policy."
                    ],
                    "durableContext": {
                        "deliveryModel": "tool_mediated",
                        "rawContextInjected": false,
                        "toolName": "project_context"
                    },
                    "workingSetSummary": {
                        "sourceRunId": "run-handoff-summary-source",
                        "activeTodoCount": 0,
                        "recentFileChangeCount": 1
                    },
                    "sourceCitedContinuityRecords": [
                        {
                            "sourceKind": "agent_message",
                            "sourceId": 10,
                            "summary": "Implemented the backend summary contract."
                        }
                    ],
                    "recentFileChanges": [
                        {
                            "path": "client/src-tauri/src/db/project_store/agent_audit.rs",
                            "operation": "edit"
                        }
                    ],
                    "toolAndCommandEvidence": [
                        {
                            "toolCallId": "tool-1",
                            "toolName": "read",
                            "state": "Succeeded"
                        }
                    ],
                    "verificationStatus": { "status": "partial", "items": ["unit test planned"] },
                    "knownRisks": [],
                    "openQuestions": [],
                    "approvedMemories": [],
                    "relevantProjectRecords": [],
                    "recentRawTailMessageReferences": [{ "messageId": 10, "role": "assistant" }],
                    "agentSpecific": { "questionBeingAnswered": "Continue after handoff." },
                    "redactionState": "redacted",
                    "redactionCount": 1,
                    "secretRawTail": "api_key=sk-s46-secret-should-not-appear"
                }),
                diagnostic: None,
                created_at: "2026-05-09T00:23:00Z".into(),
                updated_at: "2026-05-09T00:24:00Z".into(),
                completed_at: Some("2026-05-09T00:24:00Z".into()),
            },
        )
        .expect("insert handoff lineage");

        let summary = load_agent_handoff_context_summary(&repo_root, project_id, "handoff-summary")
            .expect("load handoff context summary");

        assert_eq!(
            summary["schema"],
            json!("xero.agent_handoff_context_summary.v1")
        );
        assert_eq!(
            summary["carriedContext"]["userGoal"],
            json!("Carry this work forward.")
        );
        assert_eq!(summary["redaction"]["state"], json!("redacted"));
        assert_eq!(summary["redaction"]["rawPayloadHidden"], json!(true));
        assert_eq!(summary["safetyRationale"]["sameRuntimeAgent"], json!(true));
        assert_eq!(
            summary["safetyRationale"]["sameDefinitionVersion"],
            json!(true)
        );
        assert!(summary["omittedContext"]
            .as_array()
            .expect("omitted context")
            .iter()
            .any(|entry| entry["kind"] == json!("raw_transcript")));
        let serialized = serde_json::to_string(&summary).expect("serialize summary");
        assert!(!serialized.contains("sk-s46-handoff-secret-value"));
        assert!(!serialized.contains("sk-s46-secret-should-not-appear"));
        assert!(!serialized.contains("secretRawTail"));
    }

    #[test]
    fn s64_knowledge_inspection_is_redaction_safe_and_policy_scoped() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("create repo src");
        fs::write(repo_root.join("src/lib.rs"), "pub fn knowledge() {}\n").expect("write source");
        let project_id = "project-knowledge-inspection";
        create_project_database(&repo_root, project_id);
        let definition_id = "project_researcher";
        insert_agent_definition(&repo_root, &custom_definition(definition_id))
            .expect("insert custom definition");
        let source_session = create_agent_session(
            &repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Knowledge source".into(),
                summary: String::new(),
                selected: true,
            },
        )
        .expect("create source session");
        let target_session = create_agent_session(
            &repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Knowledge target".into(),
                summary: String::new(),
                selected: false,
            },
        )
        .expect("create target session");
        insert_agent_run(
            &repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: Some(BUILTIN_AGENT_DEFINITION_VERSION),
                project_id: project_id.into(),
                agent_session_id: source_session.agent_session_id.clone(),
                run_id: "run-knowledge-source".into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Seed source run.".into(),
                system_prompt: "system".into(),
                now: "2026-05-09T00:11:00Z".into(),
            },
        )
        .expect("insert source run");
        insert_agent_run(
            &repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: Some(BUILTIN_AGENT_DEFINITION_VERSION),
                project_id: project_id.into(),
                agent_session_id: target_session.agent_session_id.clone(),
                run_id: "run-knowledge-target".into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Seed target run.".into(),
                system_prompt: "system".into(),
                now: "2026-05-09T00:12:00Z".into(),
            },
        )
        .expect("insert target run");
        insert_agent_context_manifest(
            &repo_root,
            &NewAgentContextManifestRecord {
                manifest_id: "manifest-knowledge-source".into(),
                project_id: project_id.into(),
                agent_session_id: source_session.agent_session_id.clone(),
                run_id: Some("run-knowledge-source".into()),
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: definition_id.into(),
                agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                provider_id: Some("test-provider".into()),
                model_id: Some("test-model".into()),
                request_kind: AgentContextManifestRequestKind::ProviderTurn,
                policy_action: AgentContextPolicyAction::ContinueNow,
                policy_reason_code: "within_budget".into(),
                budget_tokens: Some(1000),
                estimated_tokens: 100,
                pressure: AgentContextBudgetPressure::Low,
                context_hash: "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
                    .into(),
                included_contributors: Vec::new(),
                excluded_contributors: Vec::new(),
                retrieval_query_ids: Vec::new(),
                retrieval_result_ids: Vec::new(),
                compaction_id: None,
                handoff_id: None,
                redaction_state: AgentContextRedactionState::Clean,
                manifest: json!({
                    "promptFragments": [{ "id": "runtime.base", "title": "Base" }],
                    "toolDescriptors": [{ "name": "project_context_search" }],
                    "toolExposurePlan": { "active": ["project_context_search"] },
                    "retrieval": {
                        "deliveryModel": "tool_mediated",
                        "limit": 10,
                        "recordKinds": ["project_fact", "context_note", "finding"],
                        "memoryKinds": ["decision"]
                    },
                    "handoff": { "status": "available" }
                }),
                created_at: "2026-05-09T00:12:30Z".into(),
            },
        )
        .expect("insert knowledge context manifest");
        insert_project_record(
            &repo_root,
            &knowledge_record(
                project_id,
                "record-clean-fact",
                ProjectRecordKind::ProjectFact,
                None,
                "Clean project fact visible to retrieval.",
                ProjectRecordRedactionState::Clean,
            ),
        )
        .expect("insert clean record");
        insert_project_record(
            &repo_root,
            &knowledge_record(
                project_id,
                "record-continuity",
                ProjectRecordKind::ContextNote,
                Some("xero.project_record.current_problem_continuity.v1"),
                "Current problem continuity should be grouped separately.",
                ProjectRecordRedactionState::Clean,
            ),
        )
        .expect("insert continuity record");
        insert_project_record(
            &repo_root,
            &knowledge_record(
                project_id,
                "record-redacted",
                ProjectRecordKind::Finding,
                None,
                "SECRET_SHOULD_NOT_APPEAR redacted content.",
                ProjectRecordRedactionState::Redacted,
            ),
        )
        .expect("insert redacted record");
        insert_project_record(
            &repo_root,
            &knowledge_record(
                project_id,
                "record-blocked",
                ProjectRecordKind::Finding,
                None,
                "BLOCKED_SECRET_SHOULD_NOT_APPEAR blocked content.",
                ProjectRecordRedactionState::Blocked,
            ),
        )
        .expect("insert blocked record");
        insert_project_record(
            &repo_root,
            &knowledge_record(
                project_id,
                "record-artifact-filtered",
                ProjectRecordKind::Artifact,
                None,
                "Clean artifact outside the run retrieval policy.",
                ProjectRecordRedactionState::Clean,
            ),
        )
        .expect("insert filtered artifact record");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-knowledge".into(),
                project_id: project_id.into(),
                agent_session_id: Some(source_session.agent_session_id.clone()),
                scope: AgentMemoryScope::Session,
                kind: AgentMemoryKind::Decision,
                text: "Approved memory likely to influence the agent.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(91),
                source_run_id: Some("run-knowledge-source".into()),
                source_item_ids: vec!["message:knowledge".into()],
                diagnostic: None,
                created_at: "2026-05-09T00:13:00Z".into(),
            },
        )
        .expect("insert approved memory");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-target-filtered".into(),
                project_id: project_id.into(),
                agent_session_id: Some(target_session.agent_session_id.clone()),
                scope: AgentMemoryScope::Session,
                kind: AgentMemoryKind::Decision,
                text: "Approved target-session memory should not influence the source run.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(88),
                source_run_id: Some("run-knowledge-target".into()),
                source_item_ids: vec!["message:target-knowledge".into()],
                diagnostic: None,
                created_at: "2026-05-09T00:13:30Z".into(),
            },
        )
        .expect("insert filtered target memory");
        insert_agent_handoff_lineage(
            &repo_root,
            &NewAgentHandoffLineageRecord {
                handoff_id: "handoff-target-filtered".into(),
                project_id: project_id.into(),
                source_agent_session_id: target_session.agent_session_id.clone(),
                source_run_id: "run-knowledge-target".into(),
                source_runtime_agent_id: RuntimeAgentIdDto::Ask,
                source_agent_definition_id: definition_id.into(),
                source_agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                target_agent_session_id: None,
                target_run_id: None,
                target_runtime_agent_id: RuntimeAgentIdDto::Ask,
                target_agent_definition_id: definition_id.into(),
                target_agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                source_context_hash:
                    "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff".into(),
                status: AgentHandoffLineageStatus::Completed,
                idempotency_key: "run-knowledge-target:context:ask".into(),
                handoff_record_id: None,
                bundle: json!({
                    "schema": "xero.agent_handoff.bundle.v1",
                    "secretRawTail": "UNRELATED_HANDOFF_SECRET_SHOULD_NOT_APPEAR"
                }),
                diagnostic: None,
                created_at: "2026-05-09T00:13:45Z".into(),
                updated_at: "2026-05-09T00:13:50Z".into(),
                completed_at: Some("2026-05-09T00:13:50Z".into()),
            },
        )
        .expect("insert filtered handoff lineage");
        insert_agent_handoff_lineage(
            &repo_root,
            &NewAgentHandoffLineageRecord {
                handoff_id: "handoff-knowledge".into(),
                project_id: project_id.into(),
                source_agent_session_id: source_session.agent_session_id.clone(),
                source_run_id: "run-knowledge-source".into(),
                source_runtime_agent_id: RuntimeAgentIdDto::Ask,
                source_agent_definition_id: definition_id.into(),
                source_agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                target_agent_session_id: Some(target_session.agent_session_id),
                target_run_id: Some("run-knowledge-target".into()),
                target_runtime_agent_id: RuntimeAgentIdDto::Ask,
                target_agent_definition_id: definition_id.into(),
                target_agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                source_context_hash:
                    "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into(),
                status: AgentHandoffLineageStatus::Completed,
                idempotency_key: "run-knowledge-source:context:ask".into(),
                handoff_record_id: Some("record-continuity".into()),
                bundle: json!({
                    "schema": "xero.agent_handoff.bundle.v1",
                    "secretRawTail": "HANDOFF_SECRET_SHOULD_NOT_APPEAR"
                }),
                diagnostic: None,
                created_at: "2026-05-09T00:14:00Z".into(),
                updated_at: "2026-05-09T00:15:00Z".into(),
                completed_at: Some("2026-05-09T00:15:00Z".into()),
            },
        )
        .expect("insert handoff lineage");

        let inspection = load_agent_knowledge_inspection(
            &repo_root,
            project_id,
            None,
            Some("run-knowledge-source"),
            10,
        )
        .expect("load knowledge inspection");

        assert_eq!(
            inspection["schema"],
            json!("xero.agent_knowledge_inspection.v1")
        );
        assert_eq!(
            inspection["agentSessionId"],
            json!(source_session.agent_session_id)
        );
        assert_eq!(inspection["runId"], json!("run-knowledge-source"));
        assert_eq!(
            inspection["retrievalPolicy"]["source"],
            json!("runtime_audit_export")
        );
        assert_eq!(
            inspection["retrievalPolicy"]["recordKindFilter"],
            json!(["context_note", "finding", "project_fact"])
        );
        assert_eq!(
            inspection["retrievalPolicy"]["memoryKindFilter"],
            json!(["decision"])
        );
        assert_eq!(inspection["retrievalPolicy"]["filtersApplied"], json!(true));
        assert!(inspection["projectRecords"]
            .as_array()
            .expect("project records")
            .iter()
            .any(|record| record["recordId"] == json!("record-clean-fact")));
        assert!(!inspection["projectRecords"]
            .as_array()
            .expect("project records")
            .iter()
            .any(|record| record["recordId"] == json!("record-artifact-filtered")));
        let redacted = inspection["projectRecords"]
            .as_array()
            .expect("project records")
            .iter()
            .find(|record| record["recordId"] == json!("record-redacted"))
            .expect("redacted record present as metadata");
        assert!(redacted["textPreview"].is_null());
        assert!(redacted["summary"].is_null());
        let serialized = serde_json::to_string(&inspection).expect("serialize inspection");
        assert!(!serialized.contains("SECRET_SHOULD_NOT_APPEAR"));
        assert!(!serialized.contains("BLOCKED_SECRET_SHOULD_NOT_APPEAR"));
        assert!(!serialized.contains("HANDOFF_SECRET_SHOULD_NOT_APPEAR"));
        assert!(!serialized.contains("UNRELATED_HANDOFF_SECRET_SHOULD_NOT_APPEAR"));
        assert!(inspection["continuityRecords"]
            .as_array()
            .expect("continuity records")
            .iter()
            .any(|record| record["schemaName"]
                == json!("xero.project_record.current_problem_continuity.v1")));
        assert_eq!(
            inspection["approvedMemory"][0]["memoryId"],
            json!("memory-knowledge")
        );
        assert!(!inspection["approvedMemory"]
            .as_array()
            .expect("approved memory")
            .iter()
            .any(|memory| memory["memoryId"] == json!("memory-target-filtered")));
        assert_eq!(
            inspection["handoffRecords"][0]["handoffId"],
            json!("handoff-knowledge")
        );
        assert!(!inspection["handoffRecords"]
            .as_array()
            .expect("handoff records")
            .iter()
            .any(|handoff| handoff["handoffId"] == json!("handoff-target-filtered")));
        assert!(inspection["handoffRecords"][0]["bundleKeys"]
            .as_array()
            .expect("bundle keys")
            .contains(&json!("schema")));
    }

    #[test]
    fn s55_runtime_audit_export_reconstructs_custom_agent_lifecycle_and_run_capabilities() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("create repo root");
        let project_id = "project-agent-audit-lifecycle";
        create_project_database(&repo_root, project_id);
        let definition_id = "project_researcher";
        insert_agent_definition(
            &repo_root,
            &custom_definition_version(definition_id, 1, "2026-05-09T00:01:00Z"),
        )
        .expect("insert custom definition v1");
        insert_agent_definition(
            &repo_root,
            &custom_definition_version(definition_id, 2, "2026-05-09T00:05:00Z"),
        )
        .expect("insert custom definition v2");
        let session = create_agent_session(
            &repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Lifecycle audit".into(),
                summary: String::new(),
                selected: true,
            },
        )
        .expect("create session");
        insert_agent_run(
            &repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Ask,
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: None,
                project_id: project_id.into(),
                agent_session_id: session.agent_session_id,
                run_id: "run-audit-lifecycle".into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Exercise the audited custom agent.".into(),
                system_prompt: "system".into(),
                now: "2026-05-09T00:06:00Z".into(),
            },
        )
        .expect("insert pinned run");
        record_agent_runtime_audit_event(
            &repo_root,
            &NewAgentRuntimeAuditEventRecord {
                audit_id: agent_runtime_audit_id(
                    project_id,
                    "capability_approval_granted",
                    "destructive_write",
                    "repo_cleanup",
                    "2026-05-09T00:07:00Z",
                ),
                project_id: project_id.into(),
                actor_kind: "user".into(),
                actor_id: Some("test-user".into()),
                action_kind: "capability_approval_granted".into(),
                subject_kind: "destructive_write".into(),
                subject_id: "repo_cleanup".into(),
                run_id: Some("run-audit-lifecycle".into()),
                agent_definition_id: Some(definition_id.into()),
                agent_definition_version: Some(2),
                risk_class: Some("destructive_write".into()),
                approval_action_id: Some("approval-repo-cleanup".into()),
                payload: json!({
                    "permission": capability_permission_explanation("destructive_write", "repo_cleanup"),
                    "approvedOperation": "cleanup_generated_files"
                }),
                created_at: "2026-05-09T00:07:00Z".into(),
            },
        )
        .expect("record risky capability use");
        archive_agent_definition(&repo_root, definition_id, "2026-05-09T00:08:00Z")
            .expect("archive custom definition");

        let export = export_agent_runtime_audit(&repo_root, project_id, "run-audit-lifecycle")
            .expect("export lifecycle audit");
        assert_eq!(export.agent_definition_id, definition_id);
        assert_eq!(export.agent_definition_version, 2);
        let saved_versions = export
            .audit_events
            .iter()
            .filter(|event| event.action_kind == "agent_definition_saved")
            .filter_map(|event| event.agent_definition_version)
            .collect::<Vec<_>>();
        assert_eq!(saved_versions, vec![1, 2]);
        assert!(export.audit_events.iter().any(|event| {
            event.action_kind == "agent_definition_archived"
                && event.agent_definition_id.as_deref() == Some(definition_id)
                && event.agent_definition_version == Some(2)
        }));
        assert!(export.risky_capability_approvals.iter().any(|event| {
            event.subject_kind == "destructive_write"
                && event.run_id.as_deref() == Some("run-audit-lifecycle")
                && event.approval_action_id.as_deref() == Some("approval-repo-cleanup")
                && event.payload["permission"]
                    == capability_permission_explanation("destructive_write", "repo_cleanup")
        }));
        assert!(export.capability_permission_explanations.contains(
            &capability_permission_explanation("destructive_write", "repo_cleanup")
        ));
    }
}
