use std::path::Path;

use rusqlite::{params, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, RuntimeAgentIdDto},
    db::database_path_for_repo,
};

use super::{
    agent_core::runtime_agent_id_sql_value, open_runtime_database, read_project_row,
    validate_non_empty_text,
};

const DEFAULT_COMPACT_THRESHOLD_PERCENT: u8 = 75;
const DEFAULT_HANDOFF_THRESHOLD_PERCENT: u8 = 90;
const DEFAULT_RAW_TAIL_MESSAGE_COUNT: u32 = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentContextPolicySettingsScope {
    Project,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentContextPolicyAction {
    ContinueNow,
    CompactNow,
    RecompactNow,
    HandoffNow,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentContextBudgetPressure {
    Unknown,
    Low,
    Medium,
    High,
    Over,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentContextManifestRequestKind {
    ProviderTurn,
    HandoffSource,
    Diagnostic,
    Test,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentContextRedactionState {
    Clean,
    Redacted,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentHandoffLineageStatus {
    Pending,
    Recorded,
    TargetCreated,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRetrievalSearchScope {
    ProjectRecords,
    ApprovedMemory,
    HybridContext,
    Handoffs,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRetrievalQueryStatus {
    Started,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentRetrievalResultSourceKind {
    ProjectRecord,
    ApprovedMemory,
    Handoff,
    ContextManifest,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextPolicySettingsRecord {
    pub project_id: String,
    pub scope: AgentContextPolicySettingsScope,
    pub agent_session_id: Option<String>,
    pub auto_compact_enabled: bool,
    pub auto_handoff_enabled: bool,
    pub compact_threshold_percent: u8,
    pub handoff_threshold_percent: u8,
    pub raw_tail_message_count: u32,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentContextPolicySettingsRecord {
    pub project_id: String,
    pub scope: AgentContextPolicySettingsScope,
    pub agent_session_id: Option<String>,
    pub auto_compact_enabled: bool,
    pub auto_handoff_enabled: bool,
    pub compact_threshold_percent: u8,
    pub handoff_threshold_percent: u8,
    pub raw_tail_message_count: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextPolicyInput {
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub estimated_tokens: u64,
    pub budget_tokens: Option<u64>,
    pub provider_supports_compaction: bool,
    pub active_compaction_present: bool,
    pub compaction_current: bool,
    pub settings: AgentContextPolicySettingsRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextPolicyDecision {
    pub action: AgentContextPolicyAction,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub target_runtime_agent_id: Option<RuntimeAgentIdDto>,
    pub pressure: AgentContextBudgetPressure,
    pub pressure_percent: Option<u64>,
    pub reason_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentContextManifestContributorRecord {
    pub contributor_id: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_id: Option<String>,
    pub estimated_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentContextManifestRecord {
    pub id: i64,
    pub manifest_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: Option<String>,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub request_kind: AgentContextManifestRequestKind,
    pub policy_action: AgentContextPolicyAction,
    pub policy_reason_code: String,
    pub budget_tokens: Option<u64>,
    pub estimated_tokens: u64,
    pub pressure: AgentContextBudgetPressure,
    pub context_hash: String,
    pub included_contributors: Vec<AgentContextManifestContributorRecord>,
    pub excluded_contributors: Vec<AgentContextManifestContributorRecord>,
    pub retrieval_query_ids: Vec<String>,
    pub retrieval_result_ids: Vec<String>,
    pub compaction_id: Option<String>,
    pub handoff_id: Option<String>,
    pub redaction_state: AgentContextRedactionState,
    pub manifest: JsonValue,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentContextManifestRecord {
    pub manifest_id: String,
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: Option<String>,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub request_kind: AgentContextManifestRequestKind,
    pub policy_action: AgentContextPolicyAction,
    pub policy_reason_code: String,
    pub budget_tokens: Option<u64>,
    pub estimated_tokens: u64,
    pub pressure: AgentContextBudgetPressure,
    pub context_hash: String,
    pub included_contributors: Vec<AgentContextManifestContributorRecord>,
    pub excluded_contributors: Vec<AgentContextManifestContributorRecord>,
    pub retrieval_query_ids: Vec<String>,
    pub retrieval_result_ids: Vec<String>,
    pub compaction_id: Option<String>,
    pub handoff_id: Option<String>,
    pub redaction_state: AgentContextRedactionState,
    pub manifest: JsonValue,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentHandoffLineageRecord {
    pub id: i64,
    pub handoff_id: String,
    pub project_id: String,
    pub source_agent_session_id: String,
    pub source_run_id: String,
    pub source_runtime_agent_id: RuntimeAgentIdDto,
    pub source_agent_definition_id: String,
    pub source_agent_definition_version: u32,
    pub target_agent_session_id: Option<String>,
    pub target_run_id: Option<String>,
    pub target_runtime_agent_id: RuntimeAgentIdDto,
    pub target_agent_definition_id: String,
    pub target_agent_definition_version: u32,
    pub provider_id: String,
    pub model_id: String,
    pub source_context_hash: String,
    pub status: AgentHandoffLineageStatus,
    pub idempotency_key: String,
    pub handoff_record_id: Option<String>,
    pub bundle: JsonValue,
    pub diagnostic: Option<JsonValue>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentHandoffLineageRecord {
    pub handoff_id: String,
    pub project_id: String,
    pub source_agent_session_id: String,
    pub source_run_id: String,
    pub source_runtime_agent_id: RuntimeAgentIdDto,
    pub source_agent_definition_id: String,
    pub source_agent_definition_version: u32,
    pub target_agent_session_id: Option<String>,
    pub target_run_id: Option<String>,
    pub target_runtime_agent_id: RuntimeAgentIdDto,
    pub target_agent_definition_id: String,
    pub target_agent_definition_version: u32,
    pub provider_id: String,
    pub model_id: String,
    pub source_context_hash: String,
    pub status: AgentHandoffLineageStatus,
    pub idempotency_key: String,
    pub handoff_record_id: Option<String>,
    pub bundle: JsonValue,
    pub diagnostic: Option<JsonValue>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentHandoffLineageUpdateRecord {
    pub project_id: String,
    pub handoff_id: String,
    pub target_agent_session_id: Option<String>,
    pub target_run_id: Option<String>,
    pub status: AgentHandoffLineageStatus,
    pub handoff_record_id: Option<String>,
    pub bundle: JsonValue,
    pub diagnostic: Option<JsonValue>,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRetrievalQueryLogRecord {
    pub id: i64,
    pub query_id: String,
    pub project_id: String,
    pub agent_session_id: Option<String>,
    pub run_id: Option<String>,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub query_text: String,
    pub query_hash: String,
    pub search_scope: AgentRetrievalSearchScope,
    pub filters: JsonValue,
    pub limit_count: u32,
    pub status: AgentRetrievalQueryStatus,
    pub diagnostic: Option<JsonValue>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentRetrievalQueryLogRecord {
    pub query_id: String,
    pub project_id: String,
    pub agent_session_id: Option<String>,
    pub run_id: Option<String>,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub query_text: String,
    pub search_scope: AgentRetrievalSearchScope,
    pub filters: JsonValue,
    pub limit_count: u32,
    pub status: AgentRetrievalQueryStatus,
    pub diagnostic: Option<JsonValue>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentRetrievalResultLogRecord {
    pub id: i64,
    pub project_id: String,
    pub query_id: String,
    pub result_id: String,
    pub source_kind: AgentRetrievalResultSourceKind,
    pub source_id: String,
    pub rank: u32,
    pub score: Option<f64>,
    pub snippet: String,
    pub redaction_state: AgentContextRedactionState,
    pub metadata: Option<JsonValue>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewAgentRetrievalResultLogRecord {
    pub project_id: String,
    pub query_id: String,
    pub result_id: String,
    pub source_kind: AgentRetrievalResultSourceKind,
    pub source_id: String,
    pub rank: u32,
    pub score: Option<f64>,
    pub snippet: String,
    pub redaction_state: AgentContextRedactionState,
    pub metadata: Option<JsonValue>,
    pub created_at: String,
}

impl AgentContextPolicySettingsRecord {
    pub fn project_defaults(project_id: impl Into<String>, timestamp: impl Into<String>) -> Self {
        let timestamp = timestamp.into();
        Self {
            project_id: project_id.into(),
            scope: AgentContextPolicySettingsScope::Project,
            agent_session_id: None,
            auto_compact_enabled: true,
            auto_handoff_enabled: true,
            compact_threshold_percent: DEFAULT_COMPACT_THRESHOLD_PERCENT,
            handoff_threshold_percent: DEFAULT_HANDOFF_THRESHOLD_PERCENT,
            raw_tail_message_count: DEFAULT_RAW_TAIL_MESSAGE_COUNT,
            created_at: timestamp.clone(),
            updated_at: timestamp,
        }
    }
}

pub fn load_agent_context_policy_settings(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: Option<&str>,
) -> Result<AgentContextPolicySettingsRecord, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_context_policy_settings_project_required",
    )?;
    if let Some(agent_session_id) = agent_session_id {
        validate_non_empty_text(
            agent_session_id,
            "agentSessionId",
            "agent_context_policy_settings_session_required",
        )?;
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    if let Some(agent_session_id) = agent_session_id {
        if let Some(settings) = read_policy_settings(
            &connection,
            "WHERE project_id = ?1 AND scope_kind = 'session' AND agent_session_id = ?2",
            params![project_id, agent_session_id],
        )? {
            return Ok(settings);
        }
    }

    if let Some(settings) = read_policy_settings(
        &connection,
        "WHERE project_id = ?1 AND scope_kind = 'project' AND agent_session_id IS NULL",
        params![project_id],
    )? {
        return Ok(settings);
    }

    let defaults = AgentContextPolicySettingsRecord::project_defaults(project_id, now_timestamp());
    upsert_policy_settings_on_connection(
        &connection,
        &database_path,
        &NewAgentContextPolicySettingsRecord {
            project_id: defaults.project_id.clone(),
            scope: defaults.scope.clone(),
            agent_session_id: defaults.agent_session_id.clone(),
            auto_compact_enabled: defaults.auto_compact_enabled,
            auto_handoff_enabled: defaults.auto_handoff_enabled,
            compact_threshold_percent: defaults.compact_threshold_percent,
            handoff_threshold_percent: defaults.handoff_threshold_percent,
            raw_tail_message_count: defaults.raw_tail_message_count,
            updated_at: defaults.updated_at.clone(),
        },
    )?;
    Ok(defaults)
}

pub fn upsert_agent_context_policy_settings(
    repo_root: &Path,
    record: &NewAgentContextPolicySettingsRecord,
) -> Result<AgentContextPolicySettingsRecord, CommandError> {
    validate_policy_settings(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    upsert_policy_settings_on_connection(&connection, &database_path, record)?;
    read_policy_settings_for_scope(&connection, record)?.ok_or_else(|| {
        CommandError::system_fault(
            "agent_context_policy_settings_upsert_missing",
            "Xero wrote context policy settings but could not load them back.",
        )
    })
}

pub fn evaluate_agent_context_policy(input: AgentContextPolicyInput) -> AgentContextPolicyDecision {
    let pressure_percent = input
        .budget_tokens
        .filter(|budget| *budget > 0)
        .map(|budget| {
            input
                .estimated_tokens
                .saturating_mul(100)
                .saturating_add(budget.saturating_sub(1))
                / budget
        });
    let pressure = context_pressure_from_percent(pressure_percent);
    let handoff = |reason_code: &str| AgentContextPolicyDecision {
        action: AgentContextPolicyAction::HandoffNow,
        runtime_agent_id: input.runtime_agent_id,
        target_runtime_agent_id: Some(input.runtime_agent_id),
        pressure: pressure.clone(),
        pressure_percent,
        reason_code: reason_code.into(),
    };
    let blocked = |reason_code: &str| AgentContextPolicyDecision {
        action: AgentContextPolicyAction::Blocked,
        runtime_agent_id: input.runtime_agent_id,
        target_runtime_agent_id: None,
        pressure: pressure.clone(),
        pressure_percent,
        reason_code: reason_code.into(),
    };
    let compact = |action, reason_code: &str| AgentContextPolicyDecision {
        action,
        runtime_agent_id: input.runtime_agent_id,
        target_runtime_agent_id: None,
        pressure: pressure.clone(),
        pressure_percent,
        reason_code: reason_code.into(),
    };

    let Some(percent) = pressure_percent else {
        return compact(
            AgentContextPolicyAction::ContinueNow,
            "context_budget_unknown",
        );
    };

    if percent >= u64::from(input.settings.handoff_threshold_percent) {
        if input.settings.auto_handoff_enabled {
            return handoff("handoff_threshold_reached");
        }
        return blocked("handoff_threshold_reached_but_disabled");
    }

    if percent >= u64::from(input.settings.compact_threshold_percent) {
        if input.active_compaction_present && !input.compaction_current {
            if input.settings.auto_compact_enabled && input.provider_supports_compaction {
                return compact(
                    AgentContextPolicyAction::RecompactNow,
                    "active_compaction_no_longer_protects_turn",
                );
            }
            if input.settings.auto_handoff_enabled {
                return handoff("recompaction_unavailable");
            }
            return blocked("recompaction_unavailable_and_handoff_disabled");
        }

        if !input.active_compaction_present {
            if input.settings.auto_compact_enabled && input.provider_supports_compaction {
                return compact(
                    AgentContextPolicyAction::CompactNow,
                    "compact_threshold_reached",
                );
            }
            if input.settings.auto_handoff_enabled {
                return handoff("compaction_unavailable");
            }
            return blocked("compaction_unavailable_and_handoff_disabled");
        }
    }

    compact(
        AgentContextPolicyAction::ContinueNow,
        "context_pressure_healthy",
    )
}

pub fn insert_agent_context_manifest(
    repo_root: &Path,
    record: &NewAgentContextManifestRecord,
) -> Result<AgentContextManifestRecord, CommandError> {
    validate_manifest(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    let included_json = json_string(&record.included_contributors, "includedContributors")?;
    let excluded_json = json_string(&record.excluded_contributors, "excludedContributors")?;
    let query_ids_json = json_string(&record.retrieval_query_ids, "retrievalQueryIds")?;
    let result_ids_json = json_string(&record.retrieval_result_ids, "retrievalResultIds")?;
    let manifest_json = json_string(&record.manifest, "manifest")?;

    connection
        .execute(
            r#"
            INSERT INTO agent_context_manifests (
                manifest_id,
                project_id,
                agent_session_id,
                run_id,
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                provider_id,
                model_id,
                request_kind,
                policy_action,
                policy_reason_code,
                budget_tokens,
                estimated_tokens,
                pressure,
                context_hash,
                included_contributors_json,
                excluded_contributors_json,
                retrieval_query_ids_json,
                retrieval_result_ids_json,
                compaction_id,
                handoff_id,
                redaction_state,
                manifest_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
            "#,
            params![
                record.manifest_id,
                record.project_id,
                record.agent_session_id,
                record.run_id,
                runtime_agent_id_sql_value(&record.runtime_agent_id),
                record.agent_definition_id,
                record.agent_definition_version,
                record.provider_id,
                record.model_id,
                manifest_request_kind_sql_value(&record.request_kind),
                context_policy_action_sql_value(&record.policy_action),
                record.policy_reason_code,
                record.budget_tokens,
                record.estimated_tokens,
                context_pressure_sql_value(&record.pressure),
                record.context_hash,
                included_json,
                excluded_json,
                query_ids_json,
                result_ids_json,
                record.compaction_id,
                record.handoff_id,
                redaction_state_sql_value(&record.redaction_state),
                manifest_json,
                record.created_at,
            ],
        )
        .map_err(|error| map_continuity_write_error(&database_path, "agent_context_manifest_insert_failed", error))?;

    read_agent_context_manifest_by_row_id(
        repo_root,
        &record.project_id,
        connection.last_insert_rowid(),
    )
}

pub fn get_agent_context_manifest(
    repo_root: &Path,
    project_id: &str,
    manifest_id: &str,
) -> Result<Option<AgentContextManifestRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_context_manifest_project_required",
    )?;
    validate_non_empty_text(
        manifest_id,
        "manifestId",
        "agent_context_manifest_id_required",
    )?;
    let connection = open_continuity_database(repo_root)?;
    connection
        .query_row(
            manifest_select_sql("WHERE project_id = ?1 AND manifest_id = ?2").as_str(),
            params![project_id, manifest_id],
            read_manifest_row,
        )
        .optional()
        .map_err(map_continuity_read_error)?
        .transpose()
}

pub fn list_agent_context_manifests_for_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Result<Vec<AgentContextManifestRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_context_manifest_project_required",
    )?;
    validate_non_empty_text(run_id, "runId", "agent_context_manifest_run_required")?;
    let connection = open_continuity_database(repo_root)?;
    let mut statement = connection
        .prepare(
            manifest_select_sql(
                r#"
                WHERE project_id = ?1 AND run_id = ?2
                ORDER BY created_at ASC, id ASC
                "#,
            )
            .as_str(),
        )
        .map_err(map_continuity_read_error)?;
    let rows = statement
        .query_map(params![project_id, run_id], read_manifest_row)
        .map_err(map_continuity_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_continuity_read_error)?
        .into_iter()
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn copy_agent_context_manifests_for_branch(
    transaction: &Transaction<'_>,
    database_path: &Path,
    project_id: &str,
    source_agent_session_id: &str,
    source_run_id: &str,
    target_agent_session_id: &str,
    target_run_id: &str,
    copied_compaction: Option<(&str, &str)>,
    created_at: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_context_manifest_project_required",
    )?;
    validate_non_empty_text(
        source_agent_session_id,
        "sourceAgentSessionId",
        "agent_context_manifest_session_required",
    )?;
    validate_non_empty_text(
        source_run_id,
        "sourceRunId",
        "agent_context_manifest_run_required",
    )?;
    validate_non_empty_text(
        target_agent_session_id,
        "targetAgentSessionId",
        "agent_context_manifest_session_required",
    )?;
    validate_non_empty_text(
        target_run_id,
        "targetRunId",
        "agent_context_manifest_run_required",
    )?;

    let manifests = {
        let mut statement = transaction
            .prepare(
                manifest_select_sql(
                    r#"
                    WHERE project_id = ?1
                      AND agent_session_id = ?2
                      AND run_id = ?3
                    ORDER BY created_at ASC, id ASC
                    "#,
                )
                .as_str(),
            )
            .map_err(map_continuity_read_error)?;
        let rows = statement
            .query_map(
                params![project_id, source_agent_session_id, source_run_id],
                read_manifest_row,
            )
            .map_err(map_continuity_read_error)?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(map_continuity_read_error)?
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
    };

    for manifest in manifests {
        let manifest_id = branch_manifest_id(&manifest.manifest_id, target_run_id, manifest.id);
        let compaction_id =
            remap_branch_compaction_id(manifest.compaction_id.as_deref(), copied_compaction);
        let manifest_json = branch_manifest_json(
            manifest.manifest,
            project_id,
            source_agent_session_id,
            source_run_id,
            &manifest.manifest_id,
            target_agent_session_id,
            target_run_id,
            compaction_id.as_deref(),
        );
        transaction
            .execute(
                r#"
                INSERT INTO agent_context_manifests (
                    manifest_id,
                    project_id,
                    agent_session_id,
                    run_id,
                    runtime_agent_id,
                    agent_definition_id,
                    agent_definition_version,
                    provider_id,
                    model_id,
                    request_kind,
                    policy_action,
                    policy_reason_code,
                    budget_tokens,
                    estimated_tokens,
                    pressure,
                    context_hash,
                    included_contributors_json,
                    excluded_contributors_json,
                    retrieval_query_ids_json,
                    retrieval_result_ids_json,
                    compaction_id,
                    handoff_id,
                    redaction_state,
                    manifest_json,
                    created_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25)
                "#,
                params![
                    manifest_id,
                    project_id,
                    target_agent_session_id,
                    target_run_id,
                    runtime_agent_id_sql_value(&manifest.runtime_agent_id),
                    manifest.agent_definition_id,
                    manifest.agent_definition_version,
                    manifest.provider_id,
                    manifest.model_id,
                    manifest_request_kind_sql_value(&manifest.request_kind),
                    context_policy_action_sql_value(&manifest.policy_action),
                    manifest.policy_reason_code,
                    manifest.budget_tokens,
                    manifest.estimated_tokens,
                    context_pressure_sql_value(&manifest.pressure),
                    manifest.context_hash,
                    json_string(&manifest.included_contributors, "includedContributors")?,
                    json_string(&manifest.excluded_contributors, "excludedContributors")?,
                    json_string(&manifest.retrieval_query_ids, "retrievalQueryIds")?,
                    json_string(&manifest.retrieval_result_ids, "retrievalResultIds")?,
                    compaction_id,
                    manifest.handoff_id,
                    redaction_state_sql_value(&manifest.redaction_state),
                    json_string(&manifest_json, "manifest")?,
                    created_at,
                ],
            )
            .map_err(|error| {
                map_continuity_write_error(
                    database_path,
                    "agent_context_manifest_branch_copy_failed",
                    error,
                )
            })?;
    }

    Ok(())
}

pub fn insert_agent_handoff_lineage(
    repo_root: &Path,
    record: &NewAgentHandoffLineageRecord,
) -> Result<AgentHandoffLineageRecord, CommandError> {
    validate_handoff(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    let bundle_json = json_string(&record.bundle, "bundle")?;
    let diagnostic_json = record
        .diagnostic
        .as_ref()
        .map(|diagnostic| json_string(diagnostic, "diagnostic"))
        .transpose()?;

    connection
        .execute(
            r#"
            INSERT INTO agent_handoff_lineage (
                handoff_id,
                project_id,
                source_agent_session_id,
                source_run_id,
                source_runtime_agent_id,
                source_agent_definition_id,
                source_agent_definition_version,
                target_agent_session_id,
                target_run_id,
                target_runtime_agent_id,
                target_agent_definition_id,
                target_agent_definition_version,
                provider_id,
                model_id,
                source_context_hash,
                status,
                idempotency_key,
                handoff_record_id,
                bundle_json,
                diagnostic_json,
                created_at,
                updated_at,
                completed_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23)
            ON CONFLICT(project_id, idempotency_key) DO NOTHING
            "#,
            params![
                record.handoff_id,
                record.project_id,
                record.source_agent_session_id,
                record.source_run_id,
                runtime_agent_id_sql_value(&record.source_runtime_agent_id),
                record.source_agent_definition_id,
                record.source_agent_definition_version,
                record.target_agent_session_id,
                record.target_run_id,
                runtime_agent_id_sql_value(&record.target_runtime_agent_id),
                record.target_agent_definition_id,
                record.target_agent_definition_version,
                record.provider_id,
                record.model_id,
                record.source_context_hash,
                handoff_lineage_status_sql_value(&record.status),
                record.idempotency_key,
                record.handoff_record_id,
                bundle_json,
                diagnostic_json,
                record.created_at,
                record.updated_at,
                record.completed_at,
            ],
        )
        .map_err(|error| map_continuity_write_error(&database_path, "agent_handoff_lineage_insert_failed", error))?;

    get_agent_handoff_lineage_by_idempotency_key(
        repo_root,
        &record.project_id,
        &record.idempotency_key,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "agent_handoff_lineage_insert_missing",
            "Xero wrote handoff lineage but could not load it back.",
        )
    })
}

pub fn get_agent_handoff_lineage_by_idempotency_key(
    repo_root: &Path,
    project_id: &str,
    idempotency_key: &str,
) -> Result<Option<AgentHandoffLineageRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_handoff_lineage_project_required",
    )?;
    validate_non_empty_text(
        idempotency_key,
        "idempotencyKey",
        "agent_handoff_lineage_key_required",
    )?;
    let connection = open_continuity_database(repo_root)?;
    connection
        .query_row(
            handoff_select_sql("WHERE project_id = ?1 AND idempotency_key = ?2").as_str(),
            params![project_id, idempotency_key],
            read_handoff_row,
        )
        .optional()
        .map_err(map_continuity_read_error)?
        .transpose()
}

pub fn get_agent_handoff_lineage_by_handoff_id(
    repo_root: &Path,
    project_id: &str,
    handoff_id: &str,
) -> Result<Option<AgentHandoffLineageRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_handoff_lineage_project_required",
    )?;
    validate_non_empty_text(handoff_id, "handoffId", "agent_handoff_lineage_id_required")?;
    let connection = open_continuity_database(repo_root)?;
    connection
        .query_row(
            handoff_select_sql("WHERE project_id = ?1 AND handoff_id = ?2").as_str(),
            params![project_id, handoff_id],
            read_handoff_row,
        )
        .optional()
        .map_err(map_continuity_read_error)?
        .transpose()
}

pub fn list_agent_handoff_lineage_for_source(
    repo_root: &Path,
    project_id: &str,
    source_run_id: &str,
) -> Result<Vec<AgentHandoffLineageRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_handoff_lineage_project_required",
    )?;
    validate_non_empty_text(
        source_run_id,
        "sourceRunId",
        "agent_handoff_lineage_source_run_required",
    )?;
    let connection = open_continuity_database(repo_root)?;
    let mut statement = connection
        .prepare(
            handoff_select_sql(
                r#"
                WHERE project_id = ?1
                  AND source_run_id = ?2
                ORDER BY created_at DESC, id DESC
                "#,
            )
            .as_str(),
        )
        .map_err(map_continuity_read_error)?;
    let rows = statement
        .query_map(params![project_id, source_run_id], read_handoff_row)
        .map_err(map_continuity_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_continuity_read_error)?
        .into_iter()
        .collect()
}

pub fn list_agent_handoff_lineage_by_status(
    repo_root: &Path,
    project_id: &str,
    statuses: &[AgentHandoffLineageStatus],
) -> Result<Vec<AgentHandoffLineageRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_handoff_lineage_project_required",
    )?;
    if statuses.is_empty() {
        return Ok(Vec::new());
    }
    let status_values = statuses
        .iter()
        .map(handoff_lineage_status_sql_value)
        .collect::<Vec<_>>();
    let placeholders = (0..status_values.len())
        .map(|_| "?")
        .collect::<Vec<_>>()
        .join(", ");
    let sql = handoff_select_sql(&format!(
        r#"
        WHERE project_id = ?
          AND status IN ({placeholders})
        ORDER BY updated_at ASC, id ASC
        "#
    ));
    let connection = open_continuity_database(repo_root)?;
    let mut statement = connection
        .prepare(sql.as_str())
        .map_err(map_continuity_read_error)?;
    let mut params = vec![project_id.to_string()];
    params.extend(status_values.into_iter().map(ToOwned::to_owned));
    let rows = statement
        .query_map(
            rusqlite::params_from_iter(params.iter().map(|value| value.as_str())),
            read_handoff_row,
        )
        .map_err(map_continuity_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_continuity_read_error)?
        .into_iter()
        .collect()
}

pub fn update_agent_handoff_lineage(
    repo_root: &Path,
    update: &AgentHandoffLineageUpdateRecord,
) -> Result<AgentHandoffLineageRecord, CommandError> {
    validate_handoff_update(update)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &update.project_id)?;
    let bundle_json = json_string(&update.bundle, "bundle")?;
    let diagnostic_json = update
        .diagnostic
        .as_ref()
        .map(|diagnostic| json_string(diagnostic, "diagnostic"))
        .transpose()?;

    connection
        .execute(
            r#"
            UPDATE agent_handoff_lineage
            SET target_agent_session_id = ?3,
                target_run_id = ?4,
                status = ?5,
                handoff_record_id = ?6,
                bundle_json = ?7,
                diagnostic_json = ?8,
                updated_at = ?9,
                completed_at = ?10
            WHERE project_id = ?1
              AND handoff_id = ?2
            "#,
            params![
                update.project_id,
                update.handoff_id,
                update.target_agent_session_id,
                update.target_run_id,
                handoff_lineage_status_sql_value(&update.status),
                update.handoff_record_id,
                bundle_json,
                diagnostic_json,
                update.updated_at,
                update.completed_at,
            ],
        )
        .map_err(|error| {
            map_continuity_write_error(&database_path, "agent_handoff_lineage_update_failed", error)
        })?;

    get_agent_handoff_lineage_by_handoff_id(repo_root, &update.project_id, &update.handoff_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "agent_handoff_lineage_update_missing",
                "Xero updated handoff lineage but could not load it back.",
            )
        })
}

pub fn insert_agent_retrieval_query_log(
    repo_root: &Path,
    record: &NewAgentRetrievalQueryLogRecord,
) -> Result<AgentRetrievalQueryLogRecord, CommandError> {
    validate_retrieval_query(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    let filters_json = json_string(&record.filters, "filters")?;
    let diagnostic_json = record
        .diagnostic
        .as_ref()
        .map(|diagnostic| json_string(diagnostic, "diagnostic"))
        .transpose()?;
    let query_hash = retrieval_query_hash(&record.query_text);

    connection
        .execute(
            r#"
            INSERT INTO agent_retrieval_queries (
                query_id,
                project_id,
                agent_session_id,
                run_id,
                runtime_agent_id,
                agent_definition_id,
                agent_definition_version,
                query_text,
                query_hash,
                search_scope,
                filters_json,
                limit_count,
                status,
                diagnostic_json,
                created_at,
                completed_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            "#,
            params![
                record.query_id,
                record.project_id,
                record.agent_session_id,
                record.run_id,
                runtime_agent_id_sql_value(&record.runtime_agent_id),
                record.agent_definition_id,
                record.agent_definition_version,
                record.query_text,
                query_hash,
                retrieval_scope_sql_value(&record.search_scope),
                filters_json,
                record.limit_count,
                retrieval_query_status_sql_value(&record.status),
                diagnostic_json,
                record.created_at,
                record.completed_at,
            ],
        )
        .map_err(|error| {
            map_continuity_write_error(&database_path, "agent_retrieval_query_insert_failed", error)
        })?;

    read_agent_retrieval_query_by_row_id(
        repo_root,
        &record.project_id,
        connection.last_insert_rowid(),
    )
}

pub fn insert_agent_retrieval_result_log(
    repo_root: &Path,
    record: &NewAgentRetrievalResultLogRecord,
) -> Result<AgentRetrievalResultLogRecord, CommandError> {
    validate_retrieval_result(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    let metadata_json = record
        .metadata
        .as_ref()
        .map(|metadata| json_string(metadata, "metadata"))
        .transpose()?;

    connection
        .execute(
            r#"
            INSERT INTO agent_retrieval_results (
                project_id,
                query_id,
                result_id,
                source_kind,
                source_id,
                rank,
                score,
                snippet,
                redaction_state,
                metadata_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
            "#,
            params![
                record.project_id,
                record.query_id,
                record.result_id,
                retrieval_result_source_kind_sql_value(&record.source_kind),
                record.source_id,
                record.rank,
                record.score,
                record.snippet,
                redaction_state_sql_value(&record.redaction_state),
                metadata_json,
                record.created_at,
            ],
        )
        .map_err(|error| {
            map_continuity_write_error(
                &database_path,
                "agent_retrieval_result_insert_failed",
                error,
            )
        })?;

    read_agent_retrieval_result_by_row_id(
        repo_root,
        &record.project_id,
        connection.last_insert_rowid(),
    )
}

pub fn list_agent_retrieval_results(
    repo_root: &Path,
    project_id: &str,
    query_id: &str,
) -> Result<Vec<AgentRetrievalResultLogRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_retrieval_result_project_required",
    )?;
    validate_non_empty_text(query_id, "queryId", "agent_retrieval_result_query_required")?;
    let connection = open_continuity_database(repo_root)?;
    let mut statement = connection
        .prepare(
            retrieval_result_select_sql(
                "WHERE project_id = ?1 AND query_id = ?2 ORDER BY rank ASC, id ASC",
            )
            .as_str(),
        )
        .map_err(map_continuity_read_error)?;
    let rows = statement
        .query_map(params![project_id, query_id], read_retrieval_result_row)
        .map_err(map_continuity_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_continuity_read_error)?
        .into_iter()
        .collect()
}

pub fn retrieval_query_hash(query_text: &str) -> String {
    let normalized = query_text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hasher = Sha256::new();
    hasher.update(normalized.to_lowercase().as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn context_policy_action_sql_value(action: &AgentContextPolicyAction) -> &'static str {
    match action {
        AgentContextPolicyAction::ContinueNow => "continue_now",
        AgentContextPolicyAction::CompactNow => "compact_now",
        AgentContextPolicyAction::RecompactNow => "recompact_now",
        AgentContextPolicyAction::HandoffNow => "handoff_now",
        AgentContextPolicyAction::Blocked => "blocked",
    }
}

fn upsert_policy_settings_on_connection(
    connection: &rusqlite::Connection,
    database_path: &Path,
    record: &NewAgentContextPolicySettingsRecord,
) -> Result<(), CommandError> {
    validate_policy_settings(record)?;
    let scope = policy_settings_scope_sql_value(&record.scope);
    let changed = match record.scope {
        AgentContextPolicySettingsScope::Project => connection.execute(
            r#"
            UPDATE agent_context_policy_settings
            SET auto_compact_enabled = ?2,
                auto_handoff_enabled = ?3,
                compact_threshold_percent = ?4,
                handoff_threshold_percent = ?5,
                raw_tail_message_count = ?6,
                updated_at = ?7
            WHERE project_id = ?1
              AND scope_kind = 'project'
              AND agent_session_id IS NULL
            "#,
            params![
                record.project_id,
                if record.auto_compact_enabled { 1 } else { 0 },
                if record.auto_handoff_enabled { 1 } else { 0 },
                record.compact_threshold_percent,
                record.handoff_threshold_percent,
                record.raw_tail_message_count,
                record.updated_at,
            ],
        ),
        AgentContextPolicySettingsScope::Session => connection.execute(
            r#"
            UPDATE agent_context_policy_settings
            SET auto_compact_enabled = ?3,
                auto_handoff_enabled = ?4,
                compact_threshold_percent = ?5,
                handoff_threshold_percent = ?6,
                raw_tail_message_count = ?7,
                updated_at = ?8
            WHERE project_id = ?1
              AND scope_kind = 'session'
              AND agent_session_id = ?2
            "#,
            params![
                record.project_id,
                record.agent_session_id,
                if record.auto_compact_enabled { 1 } else { 0 },
                if record.auto_handoff_enabled { 1 } else { 0 },
                record.compact_threshold_percent,
                record.handoff_threshold_percent,
                record.raw_tail_message_count,
                record.updated_at,
            ],
        ),
    }
    .map_err(|error| {
        map_continuity_write_error(
            database_path,
            "agent_context_policy_settings_update_failed",
            error,
        )
    })?;

    if changed > 0 {
        return Ok(());
    }

    connection
        .execute(
            r#"
            INSERT INTO agent_context_policy_settings (
                project_id,
                scope_kind,
                agent_session_id,
                auto_compact_enabled,
                auto_handoff_enabled,
                compact_threshold_percent,
                handoff_threshold_percent,
                raw_tail_message_count,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)
            "#,
            params![
                record.project_id,
                scope,
                record.agent_session_id,
                if record.auto_compact_enabled { 1 } else { 0 },
                if record.auto_handoff_enabled { 1 } else { 0 },
                record.compact_threshold_percent,
                record.handoff_threshold_percent,
                record.raw_tail_message_count,
                record.updated_at,
            ],
        )
        .map_err(|error| {
            map_continuity_write_error(
                database_path,
                "agent_context_policy_settings_insert_failed",
                error,
            )
        })?;
    Ok(())
}

fn read_policy_settings_for_scope(
    connection: &rusqlite::Connection,
    record: &NewAgentContextPolicySettingsRecord,
) -> Result<Option<AgentContextPolicySettingsRecord>, CommandError> {
    match record.scope {
        AgentContextPolicySettingsScope::Project => read_policy_settings(
            connection,
            "WHERE project_id = ?1 AND scope_kind = 'project' AND agent_session_id IS NULL",
            params![record.project_id],
        ),
        AgentContextPolicySettingsScope::Session => read_policy_settings(
            connection,
            "WHERE project_id = ?1 AND scope_kind = 'session' AND agent_session_id = ?2",
            params![record.project_id, record.agent_session_id],
        ),
    }
}

fn read_policy_settings<P>(
    connection: &rusqlite::Connection,
    where_clause: &str,
    params: P,
) -> Result<Option<AgentContextPolicySettingsRecord>, CommandError>
where
    P: rusqlite::Params,
{
    connection
        .query_row(
            policy_settings_select_sql(where_clause).as_str(),
            params,
            read_policy_settings_row,
        )
        .optional()
        .map_err(map_continuity_read_error)
}

fn policy_settings_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            project_id,
            scope_kind,
            agent_session_id,
            auto_compact_enabled,
            auto_handoff_enabled,
            compact_threshold_percent,
            handoff_threshold_percent,
            raw_tail_message_count,
            created_at,
            updated_at
        FROM agent_context_policy_settings
        {where_clause}
        LIMIT 1
        "#
    )
}

fn read_policy_settings_row(row: &Row<'_>) -> rusqlite::Result<AgentContextPolicySettingsRecord> {
    Ok(AgentContextPolicySettingsRecord {
        project_id: row.get(0)?,
        scope: parse_policy_settings_scope(row.get::<_, String>(1)?.as_str()),
        agent_session_id: row.get(2)?,
        auto_compact_enabled: row.get::<_, i64>(3)? == 1,
        auto_handoff_enabled: row.get::<_, i64>(4)? == 1,
        compact_threshold_percent: row_u8(row, 5)?,
        handoff_threshold_percent: row_u8(row, 6)?,
        raw_tail_message_count: row_u32(row, 7)?,
        created_at: row.get(8)?,
        updated_at: row.get(9)?,
    })
}

fn read_agent_context_manifest_by_row_id(
    repo_root: &Path,
    project_id: &str,
    row_id: i64,
) -> Result<AgentContextManifestRecord, CommandError> {
    let connection = open_continuity_database(repo_root)?;
    connection
        .query_row(
            manifest_select_sql("WHERE project_id = ?1 AND id = ?2").as_str(),
            params![project_id, row_id],
            read_manifest_row,
        )
        .optional()
        .map_err(map_continuity_read_error)?
        .transpose()?
        .ok_or_else(|| {
            CommandError::system_fault(
                "agent_context_manifest_insert_missing",
                "Xero wrote a context manifest but could not load it back.",
            )
        })
}

fn branch_manifest_id(source_manifest_id: &str, target_run_id: &str, source_row_id: i64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(source_manifest_id.as_bytes());
    hasher.update(target_run_id.as_bytes());
    hasher.update(source_row_id.to_string().as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("context-manifest:{}:fork:{}", target_run_id, &digest[..12])
}

fn remap_branch_compaction_id(
    source_compaction_id: Option<&str>,
    copied_compaction: Option<(&str, &str)>,
) -> Option<String> {
    match (source_compaction_id, copied_compaction) {
        (Some(source), Some((old_id, new_id))) if source == old_id => Some(new_id.to_string()),
        (Some(source), _) => Some(source.to_string()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn branch_manifest_json(
    mut manifest: JsonValue,
    project_id: &str,
    source_agent_session_id: &str,
    source_run_id: &str,
    source_manifest_id: &str,
    target_agent_session_id: &str,
    target_run_id: &str,
    compaction_id: Option<&str>,
) -> JsonValue {
    if let JsonValue::Object(object) = &mut manifest {
        object.insert("projectId".into(), JsonValue::String(project_id.into()));
        object.insert(
            "agentSessionId".into(),
            JsonValue::String(target_agent_session_id.into()),
        );
        object.insert("runId".into(), JsonValue::String(target_run_id.into()));
        object.insert(
            "compactionId".into(),
            compaction_id
                .map(|value| JsonValue::String(value.into()))
                .unwrap_or(JsonValue::Null),
        );
        let mut lineage = serde_json::Map::new();
        lineage.insert("kind".into(), JsonValue::String("forked_session".into()));
        lineage.insert(
            "sourceAgentSessionId".into(),
            JsonValue::String(source_agent_session_id.into()),
        );
        lineage.insert(
            "sourceRunId".into(),
            JsonValue::String(source_run_id.into()),
        );
        lineage.insert(
            "sourceManifestId".into(),
            JsonValue::String(source_manifest_id.into()),
        );
        object.insert("lineage".into(), JsonValue::Object(lineage));
    }
    manifest
}

fn manifest_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            manifest_id,
            project_id,
            agent_session_id,
            run_id,
            runtime_agent_id,
            agent_definition_id,
            agent_definition_version,
            provider_id,
            model_id,
            request_kind,
            policy_action,
            policy_reason_code,
            budget_tokens,
            estimated_tokens,
            pressure,
            context_hash,
            included_contributors_json,
            excluded_contributors_json,
            retrieval_query_ids_json,
            retrieval_result_ids_json,
            compaction_id,
            handoff_id,
            redaction_state,
            manifest_json,
            created_at
        FROM agent_context_manifests
        {where_clause}
        "#
    )
}

fn read_manifest_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentContextManifestRecord, CommandError>> {
    Ok(Ok(AgentContextManifestRecord {
        id: row.get(0)?,
        manifest_id: row.get(1)?,
        project_id: row.get(2)?,
        agent_session_id: row.get(3)?,
        run_id: row.get(4)?,
        runtime_agent_id: parse_runtime_agent_id(row.get::<_, String>(5)?.as_str()),
        agent_definition_id: row.get(6)?,
        agent_definition_version: row_u32(row, 7)?,
        provider_id: row.get(8)?,
        model_id: row.get(9)?,
        request_kind: parse_manifest_request_kind(row.get::<_, String>(10)?.as_str()),
        policy_action: parse_context_policy_action(row.get::<_, String>(11)?.as_str()),
        policy_reason_code: row.get(12)?,
        budget_tokens: row_optional_u64(row, 13)?,
        estimated_tokens: row_u64(row, 14)?,
        pressure: parse_context_pressure(row.get::<_, String>(15)?.as_str()),
        context_hash: row.get(16)?,
        included_contributors: parse_json_column(row, 17)?,
        excluded_contributors: parse_json_column(row, 18)?,
        retrieval_query_ids: parse_json_column(row, 19)?,
        retrieval_result_ids: parse_json_column(row, 20)?,
        compaction_id: row.get(21)?,
        handoff_id: row.get(22)?,
        redaction_state: parse_redaction_state(row.get::<_, String>(23)?.as_str()),
        manifest: parse_json_column(row, 24)?,
        created_at: row.get(25)?,
    }))
}

fn handoff_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            handoff_id,
            project_id,
            source_agent_session_id,
            source_run_id,
            source_runtime_agent_id,
            source_agent_definition_id,
            source_agent_definition_version,
            target_agent_session_id,
            target_run_id,
            target_runtime_agent_id,
            target_agent_definition_id,
            target_agent_definition_version,
            provider_id,
            model_id,
            source_context_hash,
            status,
            idempotency_key,
            handoff_record_id,
            bundle_json,
            diagnostic_json,
            created_at,
            updated_at,
            completed_at
        FROM agent_handoff_lineage
        {where_clause}
        "#
    )
}

fn read_handoff_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentHandoffLineageRecord, CommandError>> {
    Ok(Ok(AgentHandoffLineageRecord {
        id: row.get(0)?,
        handoff_id: row.get(1)?,
        project_id: row.get(2)?,
        source_agent_session_id: row.get(3)?,
        source_run_id: row.get(4)?,
        source_runtime_agent_id: parse_runtime_agent_id(row.get::<_, String>(5)?.as_str()),
        source_agent_definition_id: row.get(6)?,
        source_agent_definition_version: row_u32(row, 7)?,
        target_agent_session_id: row.get(8)?,
        target_run_id: row.get(9)?,
        target_runtime_agent_id: parse_runtime_agent_id(row.get::<_, String>(10)?.as_str()),
        target_agent_definition_id: row.get(11)?,
        target_agent_definition_version: row_u32(row, 12)?,
        provider_id: row.get(13)?,
        model_id: row.get(14)?,
        source_context_hash: row.get(15)?,
        status: parse_handoff_lineage_status(row.get::<_, String>(16)?.as_str()),
        idempotency_key: row.get(17)?,
        handoff_record_id: row.get(18)?,
        bundle: parse_json_column(row, 19)?,
        diagnostic: parse_optional_json_column(row, 20)?,
        created_at: row.get(21)?,
        updated_at: row.get(22)?,
        completed_at: row.get(23)?,
    }))
}

fn read_agent_retrieval_query_by_row_id(
    repo_root: &Path,
    project_id: &str,
    row_id: i64,
) -> Result<AgentRetrievalQueryLogRecord, CommandError> {
    let connection = open_continuity_database(repo_root)?;
    connection
        .query_row(
            retrieval_query_select_sql("WHERE project_id = ?1 AND id = ?2").as_str(),
            params![project_id, row_id],
            read_retrieval_query_row,
        )
        .optional()
        .map_err(map_continuity_read_error)?
        .transpose()?
        .ok_or_else(|| {
            CommandError::system_fault(
                "agent_retrieval_query_insert_missing",
                "Xero wrote a retrieval query log but could not load it back.",
            )
        })
}

fn retrieval_query_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            query_id,
            project_id,
            agent_session_id,
            run_id,
            runtime_agent_id,
            agent_definition_id,
            agent_definition_version,
            query_text,
            query_hash,
            search_scope,
            filters_json,
            limit_count,
            status,
            diagnostic_json,
            created_at,
            completed_at
        FROM agent_retrieval_queries
        {where_clause}
        "#
    )
}

fn read_retrieval_query_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentRetrievalQueryLogRecord, CommandError>> {
    Ok(Ok(AgentRetrievalQueryLogRecord {
        id: row.get(0)?,
        query_id: row.get(1)?,
        project_id: row.get(2)?,
        agent_session_id: row.get(3)?,
        run_id: row.get(4)?,
        runtime_agent_id: parse_runtime_agent_id(row.get::<_, String>(5)?.as_str()),
        agent_definition_id: row.get(6)?,
        agent_definition_version: row_u32(row, 7)?,
        query_text: row.get(8)?,
        query_hash: row.get(9)?,
        search_scope: parse_retrieval_scope(row.get::<_, String>(10)?.as_str()),
        filters: parse_json_column(row, 11)?,
        limit_count: row_u32(row, 12)?,
        status: parse_retrieval_query_status(row.get::<_, String>(13)?.as_str()),
        diagnostic: parse_optional_json_column(row, 14)?,
        created_at: row.get(15)?,
        completed_at: row.get(16)?,
    }))
}

fn read_agent_retrieval_result_by_row_id(
    repo_root: &Path,
    project_id: &str,
    row_id: i64,
) -> Result<AgentRetrievalResultLogRecord, CommandError> {
    let connection = open_continuity_database(repo_root)?;
    connection
        .query_row(
            retrieval_result_select_sql("WHERE project_id = ?1 AND id = ?2").as_str(),
            params![project_id, row_id],
            read_retrieval_result_row,
        )
        .optional()
        .map_err(map_continuity_read_error)?
        .transpose()?
        .ok_or_else(|| {
            CommandError::system_fault(
                "agent_retrieval_result_insert_missing",
                "Xero wrote a retrieval result log but could not load it back.",
            )
        })
}

fn retrieval_result_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            project_id,
            query_id,
            result_id,
            source_kind,
            source_id,
            rank,
            score,
            snippet,
            redaction_state,
            metadata_json,
            created_at
        FROM agent_retrieval_results
        {where_clause}
        "#
    )
}

fn read_retrieval_result_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentRetrievalResultLogRecord, CommandError>> {
    Ok(Ok(AgentRetrievalResultLogRecord {
        id: row.get(0)?,
        project_id: row.get(1)?,
        query_id: row.get(2)?,
        result_id: row.get(3)?,
        source_kind: parse_retrieval_result_source_kind(row.get::<_, String>(4)?.as_str()),
        source_id: row.get(5)?,
        rank: row_u32(row, 6)?,
        score: row.get(7)?,
        snippet: row.get(8)?,
        redaction_state: parse_redaction_state(row.get::<_, String>(9)?.as_str()),
        metadata: parse_optional_json_column(row, 10)?,
        created_at: row.get(11)?,
    }))
}

fn validate_policy_settings(
    record: &NewAgentContextPolicySettingsRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_context_policy_settings_project_required",
    )?;
    match record.scope {
        AgentContextPolicySettingsScope::Project => {
            if record.agent_session_id.is_some() {
                return Err(CommandError::user_fixable(
                    "agent_context_policy_settings_scope_invalid",
                    "Project-scoped context policy settings cannot include an agent session id.",
                ));
            }
        }
        AgentContextPolicySettingsScope::Session => {
            validate_non_empty_text(
                record.agent_session_id.as_deref().unwrap_or_default(),
                "agentSessionId",
                "agent_context_policy_settings_session_required",
            )?;
        }
    }
    validate_thresholds(
        record.compact_threshold_percent,
        record.handoff_threshold_percent,
    )?;
    if record.raw_tail_message_count > 100 {
        return Err(CommandError::user_fixable(
            "agent_context_policy_settings_tail_invalid",
            "Context policy raw tail message count must be no greater than 100.",
        ));
    }
    validate_non_empty_text(
        &record.updated_at,
        "updatedAt",
        "agent_context_policy_settings_updated_at_required",
    )
}

fn validate_manifest(record: &NewAgentContextManifestRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.manifest_id,
        "manifestId",
        "agent_context_manifest_id_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_context_manifest_project_required",
    )?;
    validate_non_empty_text(
        &record.agent_session_id,
        "agentSessionId",
        "agent_context_manifest_session_required",
    )?;
    validate_optional_non_empty(
        &record.run_id,
        "runId",
        "agent_context_manifest_run_invalid",
    )?;
    validate_non_empty_text(
        &record.agent_definition_id,
        "agentDefinitionId",
        "agent_context_manifest_definition_required",
    )?;
    if record.agent_definition_version == 0 {
        return Err(CommandError::invalid_request("agentDefinitionVersion"));
    }
    validate_optional_non_empty(
        &record.provider_id,
        "providerId",
        "agent_context_manifest_provider_invalid",
    )?;
    validate_optional_non_empty(
        &record.model_id,
        "modelId",
        "agent_context_manifest_model_invalid",
    )?;
    validate_non_empty_text(
        &record.policy_reason_code,
        "policyReasonCode",
        "agent_context_manifest_policy_reason_required",
    )?;
    validate_sha256(
        &record.context_hash,
        "contextHash",
        "agent_context_manifest_hash_invalid",
    )?;
    validate_optional_non_empty(
        &record.compaction_id,
        "compactionId",
        "agent_context_manifest_compaction_invalid",
    )?;
    validate_optional_non_empty(
        &record.handoff_id,
        "handoffId",
        "agent_context_manifest_handoff_invalid",
    )?;
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_context_manifest_created_at_required",
    )
}

fn validate_handoff(record: &NewAgentHandoffLineageRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.handoff_id,
        "handoffId",
        "agent_handoff_lineage_id_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_handoff_lineage_project_required",
    )?;
    validate_non_empty_text(
        &record.source_agent_session_id,
        "sourceAgentSessionId",
        "agent_handoff_lineage_source_session_required",
    )?;
    validate_non_empty_text(
        &record.source_run_id,
        "sourceRunId",
        "agent_handoff_lineage_source_run_required",
    )?;
    if record.source_agent_definition_id != record.target_agent_definition_id
        || record.source_agent_definition_version != record.target_agent_definition_version
    {
        return Err(CommandError::user_fixable(
            "agent_handoff_lineage_target_definition_mismatch",
            "Same-agent handoff requires the target agent definition id and version to match the source definition.",
        ));
    }
    validate_non_empty_text(
        &record.source_agent_definition_id,
        "sourceAgentDefinitionId",
        "agent_handoff_lineage_source_definition_required",
    )?;
    validate_non_empty_text(
        &record.target_agent_definition_id,
        "targetAgentDefinitionId",
        "agent_handoff_lineage_target_definition_required",
    )?;
    if record.source_agent_definition_version == 0 || record.target_agent_definition_version == 0 {
        return Err(CommandError::invalid_request("agentDefinitionVersion"));
    }
    validate_optional_non_empty(
        &record.target_agent_session_id,
        "targetAgentSessionId",
        "agent_handoff_lineage_target_session_invalid",
    )?;
    validate_optional_non_empty(
        &record.target_run_id,
        "targetRunId",
        "agent_handoff_lineage_target_run_invalid",
    )?;
    validate_non_empty_text(
        &record.provider_id,
        "providerId",
        "agent_handoff_lineage_provider_required",
    )?;
    validate_non_empty_text(
        &record.model_id,
        "modelId",
        "agent_handoff_lineage_model_required",
    )?;
    validate_sha256(
        &record.source_context_hash,
        "sourceContextHash",
        "agent_handoff_lineage_context_hash_invalid",
    )?;
    validate_non_empty_text(
        &record.idempotency_key,
        "idempotencyKey",
        "agent_handoff_lineage_idempotency_key_required",
    )?;
    validate_optional_non_empty(
        &record.handoff_record_id,
        "handoffRecordId",
        "agent_handoff_lineage_record_invalid",
    )?;
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_handoff_lineage_created_at_required",
    )?;
    validate_non_empty_text(
        &record.updated_at,
        "updatedAt",
        "agent_handoff_lineage_updated_at_required",
    )?;
    validate_optional_non_empty(
        &record.completed_at,
        "completedAt",
        "agent_handoff_lineage_completed_at_invalid",
    )
}

fn validate_handoff_update(update: &AgentHandoffLineageUpdateRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &update.project_id,
        "projectId",
        "agent_handoff_lineage_project_required",
    )?;
    validate_non_empty_text(
        &update.handoff_id,
        "handoffId",
        "agent_handoff_lineage_id_required",
    )?;
    validate_optional_non_empty(
        &update.target_agent_session_id,
        "targetAgentSessionId",
        "agent_handoff_lineage_target_session_invalid",
    )?;
    validate_optional_non_empty(
        &update.target_run_id,
        "targetRunId",
        "agent_handoff_lineage_target_run_invalid",
    )?;
    validate_optional_non_empty(
        &update.handoff_record_id,
        "handoffRecordId",
        "agent_handoff_lineage_record_invalid",
    )?;
    validate_non_empty_text(
        &update.updated_at,
        "updatedAt",
        "agent_handoff_lineage_updated_at_required",
    )?;
    validate_optional_non_empty(
        &update.completed_at,
        "completedAt",
        "agent_handoff_lineage_completed_at_invalid",
    )
}

fn validate_retrieval_query(record: &NewAgentRetrievalQueryLogRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.query_id,
        "queryId",
        "agent_retrieval_query_id_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_retrieval_query_project_required",
    )?;
    validate_optional_non_empty(
        &record.agent_session_id,
        "agentSessionId",
        "agent_retrieval_query_session_invalid",
    )?;
    validate_optional_non_empty(&record.run_id, "runId", "agent_retrieval_query_run_invalid")?;
    validate_non_empty_text(
        &record.agent_definition_id,
        "agentDefinitionId",
        "agent_retrieval_query_definition_required",
    )?;
    if record.agent_definition_version == 0 {
        return Err(CommandError::invalid_request("agentDefinitionVersion"));
    }
    validate_non_empty_text(
        &record.query_text,
        "queryText",
        "agent_retrieval_query_text_required",
    )?;
    if record.limit_count == 0 {
        return Err(CommandError::user_fixable(
            "agent_retrieval_query_limit_invalid",
            "Retrieval query limit must be greater than zero.",
        ));
    }
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_retrieval_query_created_at_required",
    )?;
    validate_optional_non_empty(
        &record.completed_at,
        "completedAt",
        "agent_retrieval_query_completed_at_invalid",
    )
}

fn validate_retrieval_result(
    record: &NewAgentRetrievalResultLogRecord,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_retrieval_result_project_required",
    )?;
    validate_non_empty_text(
        &record.query_id,
        "queryId",
        "agent_retrieval_result_query_required",
    )?;
    validate_non_empty_text(
        &record.result_id,
        "resultId",
        "agent_retrieval_result_id_required",
    )?;
    validate_non_empty_text(
        &record.source_id,
        "sourceId",
        "agent_retrieval_result_source_required",
    )?;
    if record.rank == 0 {
        return Err(CommandError::user_fixable(
            "agent_retrieval_result_rank_invalid",
            "Retrieval result rank must be greater than zero.",
        ));
    }
    if record.score.is_some_and(|score| score < 0.0) {
        return Err(CommandError::user_fixable(
            "agent_retrieval_result_score_invalid",
            "Retrieval result score must be zero or greater.",
        ));
    }
    validate_non_empty_text(
        &record.snippet,
        "snippet",
        "agent_retrieval_result_snippet_required",
    )?;
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_retrieval_result_created_at_required",
    )
}

fn validate_thresholds(compact_threshold: u8, handoff_threshold: u8) -> Result<(), CommandError> {
    if !(1..=100).contains(&compact_threshold) || !(1..=100).contains(&handoff_threshold) {
        return Err(CommandError::user_fixable(
            "agent_context_policy_settings_threshold_invalid",
            "Context policy thresholds must be percentages between 1 and 100.",
        ));
    }
    if compact_threshold >= handoff_threshold {
        return Err(CommandError::user_fixable(
            "agent_context_policy_settings_threshold_order_invalid",
            "Context policy compact threshold must be lower than the handoff threshold.",
        ));
    }
    Ok(())
}

fn validate_optional_non_empty(
    value: &Option<String>,
    field: &str,
    code: &str,
) -> Result<(), CommandError> {
    if value
        .as_deref()
        .is_some_and(|value| value.trim().is_empty())
    {
        return Err(CommandError::user_fixable(
            code,
            format!("Field `{field}` must be omitted or a non-empty string."),
        ));
    }
    Ok(())
}

fn validate_sha256(value: &str, field: &str, code: &str) -> Result<(), CommandError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    Err(CommandError::user_fixable(
        code,
        format!("Field `{field}` must be a lowercase SHA-256 hash."),
    ))
}

fn json_string<T: Serialize>(value: &T, field: &str) -> Result<String, CommandError> {
    serde_json::to_string(value).map_err(|error| {
        CommandError::system_fault(
            "agent_continuity_json_serialize_failed",
            format!("Xero could not serialize `{field}` for agent continuity state: {error}"),
        )
    })
}

fn parse_json_column<T: serde::de::DeserializeOwned>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<T> {
    let raw: String = row.get(index)?;
    serde_json::from_str(&raw).map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Text,
            Box::new(error),
        )
    })
}

fn parse_optional_json_column<T: serde::de::DeserializeOwned>(
    row: &Row<'_>,
    index: usize,
) -> rusqlite::Result<Option<T>> {
    let raw: Option<String> = row.get(index)?;
    raw.as_deref()
        .map(serde_json::from_str)
        .transpose()
        .map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                index,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })
}

fn row_u64(row: &Row<'_>, index: usize) -> rusqlite::Result<u64> {
    let value: i64 = row.get(index)?;
    u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn row_optional_u64(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<u64>> {
    let value: Option<i64> = row.get(index)?;
    value
        .map(|value| {
            u64::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
        })
        .transpose()
}

fn row_u32(row: &Row<'_>, index: usize) -> rusqlite::Result<u32> {
    let value: i64 = row.get(index)?;
    u32::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn row_u8(row: &Row<'_>, index: usize) -> rusqlite::Result<u8> {
    let value: i64 = row.get(index)?;
    u8::try_from(value).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(index, value))
}

fn context_pressure_from_percent(percent: Option<u64>) -> AgentContextBudgetPressure {
    match percent {
        None => AgentContextBudgetPressure::Unknown,
        Some(0..=49) => AgentContextBudgetPressure::Low,
        Some(50..=74) => AgentContextBudgetPressure::Medium,
        Some(75..=100) => AgentContextBudgetPressure::High,
        Some(_) => AgentContextBudgetPressure::Over,
    }
}

fn open_continuity_database(repo_root: &Path) -> Result<rusqlite::Connection, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    open_runtime_database(repo_root, &database_path)
}

fn parse_runtime_agent_id(value: &str) -> RuntimeAgentIdDto {
    match value {
        "engineer" => RuntimeAgentIdDto::Engineer,
        "debug" => RuntimeAgentIdDto::Debug,
        "agent_create" => RuntimeAgentIdDto::AgentCreate,
        "test" => RuntimeAgentIdDto::Test,
        _ => RuntimeAgentIdDto::Ask,
    }
}

fn policy_settings_scope_sql_value(scope: &AgentContextPolicySettingsScope) -> &'static str {
    match scope {
        AgentContextPolicySettingsScope::Project => "project",
        AgentContextPolicySettingsScope::Session => "session",
    }
}

fn parse_policy_settings_scope(value: &str) -> AgentContextPolicySettingsScope {
    match value {
        "session" => AgentContextPolicySettingsScope::Session,
        _ => AgentContextPolicySettingsScope::Project,
    }
}

fn context_pressure_sql_value(pressure: &AgentContextBudgetPressure) -> &'static str {
    match pressure {
        AgentContextBudgetPressure::Unknown => "unknown",
        AgentContextBudgetPressure::Low => "low",
        AgentContextBudgetPressure::Medium => "medium",
        AgentContextBudgetPressure::High => "high",
        AgentContextBudgetPressure::Over => "over",
    }
}

fn parse_context_pressure(value: &str) -> AgentContextBudgetPressure {
    match value {
        "low" => AgentContextBudgetPressure::Low,
        "medium" => AgentContextBudgetPressure::Medium,
        "high" => AgentContextBudgetPressure::High,
        "over" => AgentContextBudgetPressure::Over,
        _ => AgentContextBudgetPressure::Unknown,
    }
}

fn parse_context_policy_action(value: &str) -> AgentContextPolicyAction {
    match value {
        "compact_now" => AgentContextPolicyAction::CompactNow,
        "recompact_now" => AgentContextPolicyAction::RecompactNow,
        "handoff_now" => AgentContextPolicyAction::HandoffNow,
        "blocked" => AgentContextPolicyAction::Blocked,
        _ => AgentContextPolicyAction::ContinueNow,
    }
}

fn manifest_request_kind_sql_value(kind: &AgentContextManifestRequestKind) -> &'static str {
    match kind {
        AgentContextManifestRequestKind::ProviderTurn => "provider_turn",
        AgentContextManifestRequestKind::HandoffSource => "handoff_source",
        AgentContextManifestRequestKind::Diagnostic => "diagnostic",
        AgentContextManifestRequestKind::Test => "test",
    }
}

fn parse_manifest_request_kind(value: &str) -> AgentContextManifestRequestKind {
    match value {
        "handoff_source" => AgentContextManifestRequestKind::HandoffSource,
        "diagnostic" => AgentContextManifestRequestKind::Diagnostic,
        "test" => AgentContextManifestRequestKind::Test,
        _ => AgentContextManifestRequestKind::ProviderTurn,
    }
}

fn redaction_state_sql_value(state: &AgentContextRedactionState) -> &'static str {
    match state {
        AgentContextRedactionState::Clean => "clean",
        AgentContextRedactionState::Redacted => "redacted",
        AgentContextRedactionState::Blocked => "blocked",
    }
}

fn parse_redaction_state(value: &str) -> AgentContextRedactionState {
    match value {
        "redacted" => AgentContextRedactionState::Redacted,
        "blocked" => AgentContextRedactionState::Blocked,
        _ => AgentContextRedactionState::Clean,
    }
}

fn handoff_lineage_status_sql_value(status: &AgentHandoffLineageStatus) -> &'static str {
    match status {
        AgentHandoffLineageStatus::Pending => "pending",
        AgentHandoffLineageStatus::Recorded => "recorded",
        AgentHandoffLineageStatus::TargetCreated => "target_created",
        AgentHandoffLineageStatus::Completed => "completed",
        AgentHandoffLineageStatus::Failed => "failed",
    }
}

fn parse_handoff_lineage_status(value: &str) -> AgentHandoffLineageStatus {
    match value {
        "recorded" => AgentHandoffLineageStatus::Recorded,
        "target_created" => AgentHandoffLineageStatus::TargetCreated,
        "completed" => AgentHandoffLineageStatus::Completed,
        "failed" => AgentHandoffLineageStatus::Failed,
        _ => AgentHandoffLineageStatus::Pending,
    }
}

fn retrieval_scope_sql_value(scope: &AgentRetrievalSearchScope) -> &'static str {
    match scope {
        AgentRetrievalSearchScope::ProjectRecords => "project_records",
        AgentRetrievalSearchScope::ApprovedMemory => "approved_memory",
        AgentRetrievalSearchScope::HybridContext => "hybrid_context",
        AgentRetrievalSearchScope::Handoffs => "handoffs",
    }
}

fn parse_retrieval_scope(value: &str) -> AgentRetrievalSearchScope {
    match value {
        "approved_memory" => AgentRetrievalSearchScope::ApprovedMemory,
        "hybrid_context" => AgentRetrievalSearchScope::HybridContext,
        "handoffs" => AgentRetrievalSearchScope::Handoffs,
        _ => AgentRetrievalSearchScope::ProjectRecords,
    }
}

fn retrieval_query_status_sql_value(status: &AgentRetrievalQueryStatus) -> &'static str {
    match status {
        AgentRetrievalQueryStatus::Started => "started",
        AgentRetrievalQueryStatus::Succeeded => "succeeded",
        AgentRetrievalQueryStatus::Failed => "failed",
    }
}

fn parse_retrieval_query_status(value: &str) -> AgentRetrievalQueryStatus {
    match value {
        "succeeded" => AgentRetrievalQueryStatus::Succeeded,
        "failed" => AgentRetrievalQueryStatus::Failed,
        _ => AgentRetrievalQueryStatus::Started,
    }
}

fn retrieval_result_source_kind_sql_value(kind: &AgentRetrievalResultSourceKind) -> &'static str {
    match kind {
        AgentRetrievalResultSourceKind::ProjectRecord => "project_record",
        AgentRetrievalResultSourceKind::ApprovedMemory => "approved_memory",
        AgentRetrievalResultSourceKind::Handoff => "handoff",
        AgentRetrievalResultSourceKind::ContextManifest => "context_manifest",
    }
}

fn parse_retrieval_result_source_kind(value: &str) -> AgentRetrievalResultSourceKind {
    match value {
        "approved_memory" => AgentRetrievalResultSourceKind::ApprovedMemory,
        "handoff" => AgentRetrievalResultSourceKind::Handoff,
        "context_manifest" => AgentRetrievalResultSourceKind::ContextManifest,
        _ => AgentRetrievalResultSourceKind::ProjectRecord,
    }
}

fn map_continuity_read_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "agent_continuity_read_failed",
        format!("Xero could not read agent continuity state: {error}"),
    )
}

fn map_continuity_write_error(
    database_path: &Path,
    code: &str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::retryable(
        code,
        format!(
            "Xero could not write agent continuity state in {}: {error}",
            database_path.display()
        ),
    )
}
