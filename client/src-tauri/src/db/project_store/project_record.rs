use std::{collections::BTreeSet, path::Path};

use rand::RngCore;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, RuntimeAgentIdDto},
    db::database_path_for_repo,
};

use super::{
    agent_embeddings::embedding_for_storage,
    freshness::{
        capture_source_fingerprints, evaluate_freshness, freshness_update_changed,
        parse_freshness_state, source_fingerprint_paths, source_fingerprint_paths_overlap,
        CapturedSourceFingerprints, FreshnessRefreshSummary, FreshnessState,
        SourceFingerprintInput, SupersessionUpdate,
    },
    load_agent_file_changes, open_runtime_database,
    project_record_lance::{self, ProjectRecordRow},
    read_project_row, validate_non_empty_text,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecordKind {
    AgentHandoff,
    ProjectFact,
    Decision,
    Constraint,
    Plan,
    Finding,
    Verification,
    Question,
    Artifact,
    ContextNote,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecordImportance {
    Low,
    Normal,
    High,
    Critical,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecordRedactionState {
    Clean,
    Redacted,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectRecordVisibility {
    Workflow,
    Retrieval,
    MemoryCandidate,
    Diagnostic,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProjectRecordRecord {
    pub record_id: String,
    pub project_id: String,
    pub record_kind: ProjectRecordKind,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub agent_session_id: Option<String>,
    pub run_id: String,
    pub workflow_run_id: Option<String>,
    pub workflow_step_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub text: String,
    pub text_hash: String,
    pub content_json: Option<JsonValue>,
    pub content_hash: Option<String>,
    pub schema_name: Option<String>,
    pub schema_version: i32,
    pub importance: ProjectRecordImportance,
    pub confidence: Option<f64>,
    pub tags: Vec<String>,
    pub source_item_ids: Vec<String>,
    pub related_paths: Vec<String>,
    pub produced_artifact_refs: Vec<String>,
    pub redaction_state: ProjectRecordRedactionState,
    pub visibility: ProjectRecordVisibility,
    pub freshness_state: String,
    pub freshness_checked_at: Option<String>,
    pub stale_reason: Option<String>,
    pub source_fingerprints_json: String,
    pub supersedes_id: Option<String>,
    pub superseded_by_id: Option<String>,
    pub invalidated_at: Option<String>,
    pub fact_key: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewProjectRecordRecord {
    pub record_id: String,
    pub project_id: String,
    pub record_kind: ProjectRecordKind,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub agent_session_id: Option<String>,
    pub run_id: String,
    pub workflow_run_id: Option<String>,
    pub workflow_step_id: Option<String>,
    pub title: String,
    pub summary: String,
    pub text: String,
    pub content_json: Option<JsonValue>,
    pub schema_name: Option<String>,
    pub schema_version: i32,
    pub importance: ProjectRecordImportance,
    pub confidence: Option<f64>,
    pub tags: Vec<String>,
    pub source_item_ids: Vec<String>,
    pub related_paths: Vec<String>,
    pub produced_artifact_refs: Vec<String>,
    pub redaction_state: ProjectRecordRedactionState,
    pub visibility: ProjectRecordVisibility,
    pub created_at: String,
}

pub fn generate_project_record_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "project-record-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub fn project_record_text_hash(text: &str) -> String {
    let normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn project_record_content_hash(content_json: &JsonValue) -> Result<String, CommandError> {
    let bytes = serde_json::to_vec(content_json).map_err(|error| {
        CommandError::system_fault(
            "project_record_content_hash_failed",
            format!("Xero could not hash project record content: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn insert_project_record(
    repo_root: &Path,
    record: &NewProjectRecordRecord,
) -> Result<ProjectRecordRecord, CommandError> {
    validate_new_project_record(record)?;
    let store = open_store_with_project_check(repo_root, &record.project_id)?;
    let content_hash = record
        .content_json
        .as_ref()
        .map(project_record_content_hash)
        .transpose()?;
    let embedding = embedding_for_storage(&project_record_embedding_text(record))?;
    let source_fingerprints = capture_source_fingerprints(
        repo_root,
        project_record_source_inputs(repo_root, record)?,
        &record.created_at,
    )?;
    let freshness = initial_project_record_freshness(
        &record.redaction_state,
        source_fingerprints,
        &record.created_at,
    );
    let fact_key = project_record_fact_key(
        project_record_kind_sql_value(&record.record_kind),
        &record.title,
        &record.related_paths,
    );
    let row = ProjectRecordRow {
        record_id: record.record_id.clone(),
        project_id: record.project_id.clone(),
        record_kind: project_record_kind_sql_value(&record.record_kind).into(),
        runtime_agent_id: record.runtime_agent_id,
        agent_definition_id: record.agent_definition_id.clone(),
        agent_definition_version: record.agent_definition_version,
        agent_session_id: record.agent_session_id.clone(),
        run_id: record.run_id.clone(),
        workflow_run_id: record.workflow_run_id.clone(),
        workflow_step_id: record.workflow_step_id.clone(),
        title: record.title.clone(),
        summary: record.summary.clone(),
        text: record.text.clone(),
        text_hash: project_record_text_hash(&record.text),
        content_json: record
            .content_json
            .as_ref()
            .map(serde_json::to_string)
            .transpose()
            .map_err(|error| {
                CommandError::system_fault(
                    "project_record_content_serialize_failed",
                    format!("Xero could not serialize project record content: {error}"),
                )
            })?,
        content_hash,
        schema_name: record.schema_name.clone(),
        schema_version: record.schema_version,
        importance: project_record_importance_sql_value(&record.importance).into(),
        confidence: record.confidence,
        tags_json: json_array(&record.tags, "tags")?,
        source_item_ids_json: json_array(&record.source_item_ids, "sourceItemIds")?,
        related_paths_json: json_array(&record.related_paths, "relatedPaths")?,
        produced_artifact_refs_json: json_array(
            &record.produced_artifact_refs,
            "producedArtifactRefs",
        )?,
        redaction_state: project_record_redaction_state_sql_value(&record.redaction_state).into(),
        visibility: project_record_visibility_sql_value(&record.visibility).into(),
        freshness_state: freshness.freshness_state.as_str().into(),
        freshness_checked_at: freshness.freshness_checked_at,
        stale_reason: freshness.stale_reason,
        source_fingerprints_json: freshness.source_fingerprints_json,
        supersedes_id: None,
        superseded_by_id: None,
        invalidated_at: freshness.invalidated_at,
        fact_key: Some(fact_key.clone()),
        created_at: record.created_at.clone(),
        updated_at: record.created_at.clone(),
        embedding: Some(embedding.vector),
        embedding_model: Some(embedding.model),
        embedding_dimension: Some(embedding.dimension),
        embedding_version: Some(embedding.version),
    };
    let inserted = store.insert_dedup(row)?;
    if inserted.record_id == record.record_id {
        apply_project_record_supersession(&store, &inserted, &fact_key, &record.created_at)?;
    }
    row_into_record(inserted)
}

fn apply_project_record_supersession(
    store: &project_record_lance::ProjectRecordStore,
    inserted: &ProjectRecordRow,
    fact_key: &str,
    now: &str,
) -> Result<(), CommandError> {
    if inserted.visibility
        == project_record_visibility_sql_value(&ProjectRecordVisibility::MemoryCandidate)
        || inserted.redaction_state
            == project_record_redaction_state_sql_value(&ProjectRecordRedactionState::Blocked)
    {
        return Ok(());
    }
    let mut superseded_ids = Vec::new();
    for row in store.list_rows()? {
        if row.record_id == inserted.record_id
            || row.visibility
                == project_record_visibility_sql_value(&ProjectRecordVisibility::MemoryCandidate)
            || row.redaction_state
                == project_record_redaction_state_sql_value(&ProjectRecordRedactionState::Blocked)
            || row.freshness_state != FreshnessState::Current.as_str()
        {
            continue;
        }
        let row_related_paths = parse_json_array(&row.related_paths_json, "relatedPaths")?;
        let row_fact_key = row.fact_key.clone().unwrap_or_else(|| {
            project_record_fact_key(&row.record_kind, &row.title, &row_related_paths)
        });
        if row_fact_key == fact_key
            && source_fingerprint_paths_overlap(
                &row.source_fingerprints_json,
                &inserted.source_fingerprints_json,
            )?
        {
            store.update_supersession(
                &row.record_id,
                SupersessionUpdate {
                    superseded_by_id: Some(inserted.record_id.clone()),
                    supersedes_id: row.supersedes_id,
                    fact_key: Some(fact_key.to_string()),
                    invalidated_at: Some(now.to_string()),
                    stale_reason: Some(format!(
                        "Superseded by newer durable context `{}`.",
                        inserted.record_id
                    )),
                    updated_at: now.to_string(),
                },
            )?;
            superseded_ids.push(row.record_id);
        }
    }
    if let Some(supersedes_id) = superseded_ids.first() {
        store.update_supersession(
            &inserted.record_id,
            SupersessionUpdate {
                superseded_by_id: None,
                supersedes_id: Some(supersedes_id.clone()),
                fact_key: Some(fact_key.to_string()),
                invalidated_at: inserted.invalidated_at.clone(),
                stale_reason: inserted.stale_reason.clone(),
                updated_at: now.to_string(),
            },
        )?;
    }
    Ok(())
}

fn project_record_fact_key(record_kind: &str, title: &str, related_paths: &[String]) -> String {
    let mut paths = related_paths
        .iter()
        .filter_map(|path| {
            let normalized = normalize_fact_segment(path);
            (!normalized.is_empty()).then_some(normalized)
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup();
    format!(
        "record:{}:{}:{}",
        normalize_fact_segment(record_kind),
        normalize_fact_segment(title),
        paths.join(",")
    )
}

fn normalize_fact_segment(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

struct ProjectRecordInitialFreshness {
    freshness_state: FreshnessState,
    freshness_checked_at: Option<String>,
    stale_reason: Option<String>,
    source_fingerprints_json: String,
    invalidated_at: Option<String>,
}

fn project_record_source_inputs(
    repo_root: &Path,
    record: &NewProjectRecordRecord,
) -> Result<Vec<SourceFingerprintInput>, CommandError> {
    let mut inputs = record
        .related_paths
        .iter()
        .cloned()
        .map(SourceFingerprintInput::related_path)
        .collect::<Vec<_>>();
    let related_paths = record
        .related_paths
        .iter()
        .map(|path| path.as_str())
        .collect::<BTreeSet<_>>();
    for change in load_agent_file_changes(repo_root, &record.project_id, &record.run_id)? {
        if !related_paths.is_empty() && !related_paths.contains(change.path.as_str()) {
            continue;
        }
        inputs.push(SourceFingerprintInput::agent_file_change(
            change.path,
            format!("file_change:{}", change.id),
            change.operation,
        ));
    }
    Ok(inputs)
}

fn initial_project_record_freshness(
    redaction_state: &ProjectRecordRedactionState,
    source_fingerprints: CapturedSourceFingerprints,
    created_at: &str,
) -> ProjectRecordInitialFreshness {
    if *redaction_state == ProjectRecordRedactionState::Blocked {
        return ProjectRecordInitialFreshness {
            freshness_state: FreshnessState::Blocked,
            freshness_checked_at: None,
            stale_reason: None,
            source_fingerprints_json: source_fingerprints.source_fingerprints_json,
            invalidated_at: None,
        };
    }
    let invalidated_at = if source_fingerprints.freshness_state == FreshnessState::SourceMissing {
        Some(created_at.into())
    } else {
        None
    };
    ProjectRecordInitialFreshness {
        freshness_state: source_fingerprints.freshness_state,
        freshness_checked_at: source_fingerprints.freshness_checked_at,
        stale_reason: source_fingerprints.stale_reason,
        source_fingerprints_json: source_fingerprints.source_fingerprints_json,
        invalidated_at,
    }
}

pub fn list_project_records(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<ProjectRecordRecord>, CommandError> {
    let store = open_store_with_project_check(repo_root, project_id)?;
    store
        .list()?
        .into_iter()
        .map(row_into_record)
        .collect::<Result<Vec<_>, _>>()
}

pub fn refresh_all_project_record_freshness(
    repo_root: &Path,
    project_id: &str,
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let store = open_store_with_project_check(repo_root, project_id)?;
    refresh_project_record_rows(repo_root, &store, None, checked_at)
}

pub fn refresh_project_record_freshness_for_paths(
    repo_root: &Path,
    project_id: &str,
    paths: &[String],
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let store = open_store_with_project_check(repo_root, project_id)?;
    let paths = paths.iter().cloned().collect::<BTreeSet<_>>();
    refresh_project_record_rows(repo_root, &store, Some(&paths), checked_at)
}

pub fn refresh_project_record_freshness_for_ids(
    repo_root: &Path,
    project_id: &str,
    record_ids: &[String],
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let store = open_store_with_project_check(repo_root, project_id)?;
    let record_ids = record_ids.iter().cloned().collect::<BTreeSet<_>>();
    refresh_project_record_rows_for_ids(repo_root, &store, &record_ids, checked_at)
}

pub(crate) fn refresh_project_record_rows(
    repo_root: &Path,
    store: &project_record_lance::ProjectRecordStore,
    paths: Option<&BTreeSet<String>>,
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let mut summary = FreshnessRefreshSummary::default();
    for row in store.list_rows()? {
        if !row_matches_freshness_paths(&row.source_fingerprints_json, paths)? {
            continue;
        }
        let update = evaluate_freshness(
            repo_root,
            parse_freshness_state(&row.freshness_state),
            row.invalidated_at.as_deref(),
            &row.source_fingerprints_json,
            checked_at,
            row.redaction_state
                == project_record_redaction_state_sql_value(&ProjectRecordRedactionState::Blocked),
        )?;
        let changed = freshness_update_changed(
            &row.freshness_state,
            row.freshness_checked_at.as_deref(),
            row.stale_reason.as_deref(),
            &row.source_fingerprints_json,
            row.invalidated_at.as_deref(),
            &update,
        );
        summary.record_state(update.freshness_state, changed);
        if changed {
            let _ = store.update_freshness(&row.record_id, update)?;
        }
    }
    Ok(summary)
}

fn refresh_project_record_rows_for_ids(
    repo_root: &Path,
    store: &project_record_lance::ProjectRecordStore,
    record_ids: &BTreeSet<String>,
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let mut summary = FreshnessRefreshSummary::default();
    if record_ids.is_empty() {
        return Ok(summary);
    }
    for row in store.list_rows()? {
        if !record_ids.contains(&row.record_id) {
            continue;
        }
        let update = evaluate_freshness(
            repo_root,
            parse_freshness_state(&row.freshness_state),
            row.invalidated_at.as_deref(),
            &row.source_fingerprints_json,
            checked_at,
            row.redaction_state
                == project_record_redaction_state_sql_value(&ProjectRecordRedactionState::Blocked),
        )?;
        let changed = freshness_update_changed(
            &row.freshness_state,
            row.freshness_checked_at.as_deref(),
            row.stale_reason.as_deref(),
            &row.source_fingerprints_json,
            row.invalidated_at.as_deref(),
            &update,
        );
        summary.record_state(update.freshness_state, changed);
        if changed {
            let _ = store.update_freshness(&row.record_id, update)?;
        }
    }
    Ok(summary)
}

pub fn mark_project_record_superseded_by(
    repo_root: &Path,
    project_id: &str,
    superseded_record_id: &str,
    superseding_record_id: &str,
    now: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        superseded_record_id,
        "supersededRecordId",
        "project_record_superseded_id_required",
    )?;
    validate_non_empty_text(
        superseding_record_id,
        "supersedingRecordId",
        "project_record_superseding_id_required",
    )?;
    if superseded_record_id == superseding_record_id {
        return Ok(());
    }
    let store = open_store_with_project_check(repo_root, project_id)?;
    let rows = store.list_rows()?;
    let Some(superseded) = rows
        .iter()
        .find(|row| row.record_id == superseded_record_id)
    else {
        return Err(CommandError::user_fixable(
            "project_record_superseded_not_found",
            format!("Project record `{superseded_record_id}` was not found."),
        ));
    };
    let Some(superseding) = rows
        .iter()
        .find(|row| row.record_id == superseding_record_id)
    else {
        return Err(CommandError::user_fixable(
            "project_record_superseding_not_found",
            format!("Project record `{superseding_record_id}` was not found."),
        ));
    };
    let fact_key = superseding
        .fact_key
        .clone()
        .or_else(|| superseded.fact_key.clone());
    store.update_supersession(
        superseded_record_id,
        SupersessionUpdate {
            superseded_by_id: Some(superseding_record_id.to_string()),
            supersedes_id: superseded.supersedes_id.clone(),
            fact_key: fact_key.clone(),
            invalidated_at: Some(now.to_string()),
            stale_reason: Some(format!(
                "Superseded by newer durable context `{superseding_record_id}`."
            )),
            updated_at: now.to_string(),
        },
    )?;
    store.update_supersession(
        superseding_record_id,
        SupersessionUpdate {
            superseded_by_id: superseding.superseded_by_id.clone(),
            supersedes_id: Some(superseded_record_id.to_string()),
            fact_key,
            invalidated_at: superseding.invalidated_at.clone(),
            stale_reason: superseding.stale_reason.clone(),
            updated_at: now.to_string(),
        },
    )?;
    Ok(())
}

fn row_matches_freshness_paths(
    source_fingerprints_json: &str,
    paths: Option<&BTreeSet<String>>,
) -> Result<bool, CommandError> {
    let Some(paths) = paths else {
        return Ok(true);
    };
    if paths.is_empty() {
        return Ok(true);
    }
    let fingerprint_paths = source_fingerprint_paths(source_fingerprints_json)?;
    Ok(fingerprint_paths.iter().any(|fingerprint_path| {
        paths
            .iter()
            .any(|path| path == fingerprint_path || path.ends_with(fingerprint_path))
    }))
}

fn open_store_with_project_check(
    repo_root: &Path,
    project_id: &str,
) -> Result<project_record_lance::ProjectRecordStore, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "project_record_project_id_required",
    )?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    Ok(project_record_lance::open_for_database_path(
        &database_path,
        project_id,
    ))
}

fn validate_new_project_record(record: &NewProjectRecordRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.record_id,
        "recordId",
        "project_record_record_id_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "project_record_project_id_required",
    )?;
    validate_non_empty_text(
        &record.agent_definition_id,
        "agentDefinitionId",
        "project_record_agent_definition_id_required",
    )?;
    if record.agent_definition_version == 0 {
        return Err(CommandError::invalid_request("agentDefinitionVersion"));
    }
    validate_non_empty_text(&record.run_id, "runId", "project_record_run_id_required")?;
    validate_non_empty_text(&record.title, "title", "project_record_title_required")?;
    validate_non_empty_text(
        &record.summary,
        "summary",
        "project_record_summary_required",
    )?;
    validate_non_empty_text(&record.text, "text", "project_record_text_required")?;
    if record.schema_version <= 0 {
        return Err(CommandError::invalid_request("schemaVersion"));
    }
    if record
        .confidence
        .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
    {
        return Err(CommandError::invalid_request("confidence"));
    }
    Ok(())
}

fn project_record_embedding_text(record: &NewProjectRecordRecord) -> String {
    let mut text = format!("{}.\n{}\n{}", record.title, record.summary, record.text);
    if !record.tags.is_empty() {
        text.push_str("\nTags: ");
        text.push_str(&record.tags.join(", "));
    }
    if !record.related_paths.is_empty() {
        text.push_str("\nRelated paths: ");
        text.push_str(&record.related_paths.join(", "));
    }
    text
}

fn row_into_record(row: ProjectRecordRow) -> Result<ProjectRecordRecord, CommandError> {
    Ok(ProjectRecordRecord {
        record_id: row.record_id,
        project_id: row.project_id,
        record_kind: parse_project_record_kind(&row.record_kind),
        runtime_agent_id: row.runtime_agent_id,
        agent_definition_id: row.agent_definition_id,
        agent_definition_version: row.agent_definition_version,
        agent_session_id: row.agent_session_id,
        run_id: row.run_id,
        workflow_run_id: row.workflow_run_id,
        workflow_step_id: row.workflow_step_id,
        title: row.title,
        summary: row.summary,
        text: row.text,
        text_hash: row.text_hash,
        content_json: row
            .content_json
            .map(|value| serde_json::from_str(&value))
            .transpose()
            .map_err(|error| {
                CommandError::system_fault(
                    "project_record_content_decode_failed",
                    format!("Xero could not decode project record content: {error}"),
                )
            })?,
        content_hash: row.content_hash,
        schema_name: row.schema_name,
        schema_version: row.schema_version,
        importance: parse_project_record_importance(&row.importance),
        confidence: row.confidence,
        tags: parse_json_array(&row.tags_json, "tags")?,
        source_item_ids: parse_json_array(&row.source_item_ids_json, "sourceItemIds")?,
        related_paths: parse_json_array(&row.related_paths_json, "relatedPaths")?,
        produced_artifact_refs: parse_json_array(
            &row.produced_artifact_refs_json,
            "producedArtifactRefs",
        )?,
        redaction_state: parse_project_record_redaction_state(&row.redaction_state),
        visibility: parse_project_record_visibility(&row.visibility),
        freshness_state: row.freshness_state,
        freshness_checked_at: row.freshness_checked_at,
        stale_reason: row.stale_reason,
        source_fingerprints_json: row.source_fingerprints_json,
        supersedes_id: row.supersedes_id,
        superseded_by_id: row.superseded_by_id,
        invalidated_at: row.invalidated_at,
        fact_key: row.fact_key,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn json_array(values: &[String], field: &'static str) -> Result<String, CommandError> {
    serde_json::to_string(values).map_err(|error| {
        CommandError::system_fault(
            "project_record_array_serialize_failed",
            format!("Xero could not serialize project record {field}: {error}"),
        )
    })
}

fn parse_json_array(value: &str, field: &'static str) -> Result<Vec<String>, CommandError> {
    serde_json::from_str(value).map_err(|error| {
        CommandError::system_fault(
            "project_record_array_decode_failed",
            format!("Xero could not decode project record {field}: {error}"),
        )
    })
}

pub fn project_record_kind_sql_value(kind: &ProjectRecordKind) -> &'static str {
    match kind {
        ProjectRecordKind::AgentHandoff => "agent_handoff",
        ProjectRecordKind::ProjectFact => "project_fact",
        ProjectRecordKind::Decision => "decision",
        ProjectRecordKind::Constraint => "constraint",
        ProjectRecordKind::Plan => "plan",
        ProjectRecordKind::Finding => "finding",
        ProjectRecordKind::Verification => "verification",
        ProjectRecordKind::Question => "question",
        ProjectRecordKind::Artifact => "artifact",
        ProjectRecordKind::ContextNote => "context_note",
        ProjectRecordKind::Diagnostic => "diagnostic",
    }
}

fn parse_project_record_kind(value: &str) -> ProjectRecordKind {
    match value {
        "project_fact" => ProjectRecordKind::ProjectFact,
        "decision" => ProjectRecordKind::Decision,
        "constraint" => ProjectRecordKind::Constraint,
        "plan" => ProjectRecordKind::Plan,
        "finding" => ProjectRecordKind::Finding,
        "verification" => ProjectRecordKind::Verification,
        "question" => ProjectRecordKind::Question,
        "artifact" => ProjectRecordKind::Artifact,
        "context_note" => ProjectRecordKind::ContextNote,
        "diagnostic" => ProjectRecordKind::Diagnostic,
        _ => ProjectRecordKind::AgentHandoff,
    }
}

fn project_record_importance_sql_value(importance: &ProjectRecordImportance) -> &'static str {
    match importance {
        ProjectRecordImportance::Low => "low",
        ProjectRecordImportance::Normal => "normal",
        ProjectRecordImportance::High => "high",
        ProjectRecordImportance::Critical => "critical",
    }
}

fn parse_project_record_importance(value: &str) -> ProjectRecordImportance {
    match value {
        "low" => ProjectRecordImportance::Low,
        "high" => ProjectRecordImportance::High,
        "critical" => ProjectRecordImportance::Critical,
        _ => ProjectRecordImportance::Normal,
    }
}

fn project_record_redaction_state_sql_value(
    redaction_state: &ProjectRecordRedactionState,
) -> &'static str {
    match redaction_state {
        ProjectRecordRedactionState::Clean => "clean",
        ProjectRecordRedactionState::Redacted => "redacted",
        ProjectRecordRedactionState::Blocked => "blocked",
    }
}

fn parse_project_record_redaction_state(value: &str) -> ProjectRecordRedactionState {
    match value {
        "redacted" => ProjectRecordRedactionState::Redacted,
        "blocked" => ProjectRecordRedactionState::Blocked,
        _ => ProjectRecordRedactionState::Clean,
    }
}

fn project_record_visibility_sql_value(visibility: &ProjectRecordVisibility) -> &'static str {
    match visibility {
        ProjectRecordVisibility::Workflow => "workflow",
        ProjectRecordVisibility::Retrieval => "retrieval",
        ProjectRecordVisibility::MemoryCandidate => "memory_candidate",
        ProjectRecordVisibility::Diagnostic => "diagnostic",
    }
}

fn parse_project_record_visibility(value: &str) -> ProjectRecordVisibility {
    match value {
        "workflow" => ProjectRecordVisibility::Workflow,
        "memory_candidate" => ProjectRecordVisibility::MemoryCandidate,
        "diagnostic" => ProjectRecordVisibility::Diagnostic,
        _ => ProjectRecordVisibility::Retrieval,
    }
}

pub fn now_project_record_timestamp() -> String {
    now_timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use rusqlite::{params, Connection};
    use serde_json::json;

    use crate::db::project_store::{
        append_agent_file_change, create_agent_session, insert_agent_run, AgentSessionCreateRecord,
        NewAgentFileChangeRecord, NewAgentRunRecord,
    };
    use crate::db::{configure_connection, database_path_for_repo, migrations::migrations};

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
        crate::db::register_project_database_path(repo_root, &database_path);
        database_path
    }

    fn new_project_record(project_id: &str, record_id: &str, text: &str) -> NewProjectRecordRecord {
        NewProjectRecordRecord {
            record_id: record_id.into(),
            project_id: project_id.into(),
            record_kind: ProjectRecordKind::AgentHandoff,
            runtime_agent_id: RuntimeAgentIdDto::Ask,
            agent_definition_id: "ask".into(),
            agent_definition_version: crate::db::project_store::BUILTIN_AGENT_DEFINITION_VERSION,
            agent_session_id: Some("agent-session-1".into()),
            run_id: "run-1".into(),
            workflow_run_id: None,
            workflow_step_id: None,
            title: "Ask run handoff".into(),
            summary: "Ask answered a repository question.".into(),
            text: text.into(),
            content_json: Some(json!({
                "schema": "xero.project_record.run_handoff.v1",
                "facts": ["Ask inspected the project without mutating it."]
            })),
            schema_name: Some("xero.project_record.run_handoff.v1".into()),
            schema_version: 1,
            importance: ProjectRecordImportance::Normal,
            confidence: Some(1.0),
            tags: vec!["ask".into(), "handoff".into()],
            source_item_ids: vec!["message-1".into()],
            related_paths: vec!["src/main.rs".into()],
            produced_artifact_refs: Vec::new(),
            redaction_state: ProjectRecordRedactionState::Clean,
            visibility: ProjectRecordVisibility::Retrieval,
            created_at: "2026-05-01T00:00:00Z".into(),
        }
    }

    fn seed_agent_run(repo_root: &Path, project_id: &str, run_id: &str) -> String {
        let session = create_agent_session(
            repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Project record source run".into(),
                summary: String::new(),
                selected: true,
            },
        )
        .expect("create agent session");
        insert_agent_run(
            repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: None,
                agent_definition_version: None,
                project_id: project_id.into(),
                agent_session_id: session.agent_session_id.clone(),
                run_id: run_id.into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "Record durable context.".into(),
                system_prompt: "system".into(),
                now: "2026-05-03T00:00:00Z".into(),
            },
        )
        .expect("insert agent run");
        session.agent_session_id
    }

    #[test]
    fn project_records_store_under_app_data_lance_and_deduplicate() {
        project_record_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(repo_root.join("src/main.rs"), "fn main() {}\n").expect("write source");
        let project_id = "project-ask-records";
        let database_path = create_project_database(&repo_root, project_id);
        let lance_dir = project_record_lance::dataset_dir_for_database_path(&database_path);

        assert_eq!(database_path_for_repo(&repo_root), database_path);
        assert!(lance_dir.starts_with(database_path.parent().expect("database parent")));
        assert!(!repo_root.join(".xero").exists());

        let inserted = insert_project_record(
            &repo_root,
            &new_project_record(
                project_id,
                "project-record-1",
                "Ask found that the app stores project records in LanceDB.",
            ),
        )
        .expect("insert record");
        let duplicate = insert_project_record(
            &repo_root,
            &new_project_record(
                project_id,
                "project-record-2",
                "Ask found that the app stores project records in LanceDB.",
            ),
        )
        .expect("dedupe record");
        let records = list_project_records(&repo_root, project_id).expect("list records");

        assert_eq!(duplicate.record_id, inserted.record_id);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].runtime_agent_id, RuntimeAgentIdDto::Ask);
        assert_eq!(records[0].record_kind, ProjectRecordKind::AgentHandoff);
        assert_eq!(records[0].freshness_state, FreshnessState::Current.as_str());
        assert_eq!(
            records[0].freshness_checked_at.as_deref(),
            Some("2026-05-01T00:00:00Z")
        );
        let fingerprints: JsonValue =
            serde_json::from_str(&records[0].source_fingerprints_json).expect("fingerprints json");
        assert_eq!(fingerprints["fingerprints"][0]["path"], "src/main.rs");
        assert_eq!(fingerprints["fingerprints"][0]["exists"], true);
        assert!(fingerprints["fingerprints"][0]["hash"].is_string());
        assert!(lance_dir.join("project_records.lance").exists());
        assert!(!repo_root.join(".xero").exists());
    }

    #[test]
    fn project_records_mark_missing_related_paths_as_source_missing() {
        project_record_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-missing-record-source";
        create_project_database(&repo_root, project_id);

        let inserted = insert_project_record(
            &repo_root,
            &new_project_record(
                project_id,
                "project-record-missing-source",
                "Ask found that the app stores project records in LanceDB.",
            ),
        )
        .expect("insert record");
        let fingerprints: JsonValue =
            serde_json::from_str(&inserted.source_fingerprints_json).expect("fingerprints json");

        assert_eq!(
            inserted.freshness_state,
            FreshnessState::SourceMissing.as_str()
        );
        assert_eq!(
            inserted.invalidated_at.as_deref(),
            Some("2026-05-01T00:00:00Z")
        );
        assert!(inserted
            .stale_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("src/main.rs")));
        assert_eq!(fingerprints["fingerprints"][0]["path"], "src/main.rs");
        assert_eq!(fingerprints["fingerprints"][0]["exists"], false);
    }

    #[test]
    fn project_records_use_run_file_change_fingerprints_when_available() {
        project_record_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(repo_root.join("src/main.rs"), "fn main() {}\n").expect("write source");
        let project_id = "project-record-file-change-source";
        create_project_database(&repo_root, project_id);
        let agent_session_id = seed_agent_run(&repo_root, project_id, "run-record-source");
        let file_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-record-source".into(),
                path: "src/main.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:01:00Z".into(),
            },
        )
        .expect("append file change");
        let mut record = new_project_record(
            project_id,
            "project-record-file-change-source",
            "Engineer updated src/main.rs and recorded the durable context.",
        );
        record.runtime_agent_id = RuntimeAgentIdDto::Engineer;
        record.agent_definition_id = "engineer".into();
        record.agent_session_id = Some(agent_session_id);
        record.run_id = "run-record-source".into();

        let inserted = insert_project_record(&repo_root, &record).expect("insert record");
        let fingerprints: JsonValue =
            serde_json::from_str(&inserted.source_fingerprints_json).expect("fingerprints json");

        assert_eq!(inserted.freshness_state, FreshnessState::Current.as_str());
        assert_eq!(fingerprints["fingerprints"][0]["path"], "src/main.rs");
        assert_eq!(
            fingerprints["fingerprints"][0]["source"],
            "agent_file_change"
        );
        assert_eq!(
            fingerprints["fingerprints"][0]["sourceItemId"],
            format!("file_change:{}", file_change.id)
        );
    }

    #[test]
    fn project_records_with_distinct_source_paths_do_not_supersede() {
        project_record_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(repo_root.join("src/alpha.rs"), "pub fn alpha() {}\n")
            .expect("write alpha source");
        fs::write(repo_root.join("src/beta.rs"), "pub fn beta() {}\n").expect("write beta source");
        let project_id = "project-record-distinct-source-supersession";
        create_project_database(&repo_root, project_id);

        let old_session_id = seed_agent_run(&repo_root, project_id, "run-distinct-record-old");
        append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-distinct-record-old".into(),
                path: "src/alpha.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:01:00Z".into(),
            },
        )
        .expect("append old file change");
        let mut old = new_project_record(
            project_id,
            "project-record-distinct-old",
            "The shared source decision applies to alpha.",
        );
        old.title = "Shared source decision".into();
        old.related_paths = Vec::new();
        old.run_id = "run-distinct-record-old".into();
        old.agent_session_id = Some(old_session_id);
        old.created_at = "2026-05-03T00:02:00Z".into();
        insert_project_record(&repo_root, &old).expect("insert old record");

        let new_session_id = seed_agent_run(&repo_root, project_id, "run-distinct-record-new");
        append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-distinct-record-new".into(),
                path: "src/beta.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:03:00Z".into(),
            },
        )
        .expect("append new file change");
        let mut new = new_project_record(
            project_id,
            "project-record-distinct-new",
            "The shared source decision applies to beta.",
        );
        new.title = "Shared source decision".into();
        new.related_paths = Vec::new();
        new.run_id = "run-distinct-record-new".into();
        new.agent_session_id = Some(new_session_id);
        new.created_at = "2026-05-03T00:04:00Z".into();
        insert_project_record(&repo_root, &new).expect("insert new record");

        let records = list_project_records(&repo_root, project_id).expect("list records");
        let old = records
            .iter()
            .find(|record| record.record_id == "project-record-distinct-old")
            .expect("old record");
        let new = records
            .iter()
            .find(|record| record.record_id == "project-record-distinct-new")
            .expect("new record");

        assert_eq!(old.fact_key, new.fact_key);
        assert_eq!(old.freshness_state, FreshnessState::Current.as_str());
        assert_eq!(new.freshness_state, FreshnessState::Current.as_str());
        assert!(old.superseded_by_id.is_none());
        assert!(new.supersedes_id.is_none());
    }

    #[test]
    fn project_records_round_trip_debug_runtime_agent() {
        project_record_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-debug-records";
        create_project_database(&repo_root, project_id);
        let mut record = new_project_record(
            project_id,
            "project-record-debug-1",
            "Debug found the root cause, fixed it, and verified the regression test.",
        );
        record.runtime_agent_id = RuntimeAgentIdDto::Debug;
        record.title = "Debug run handoff".into();
        record.summary = "Debug found the root cause and verified the fix.".into();
        record.schema_name = Some("xero.project_record.debug_session.v1".into());
        record.content_json = Some(json!({
            "schema": "xero.project_record.debug_session.v1",
            "debugSession": {
                "memoryFocus": ["rootCause", "fix", "verification"]
            }
        }));
        record.importance = ProjectRecordImportance::High;
        record.tags = vec![
            "debug".into(),
            "debugging".into(),
            "root-cause".into(),
            "verification".into(),
        ];

        let inserted = insert_project_record(&repo_root, &record).expect("insert debug record");
        let records = list_project_records(&repo_root, project_id).expect("list records");

        assert_eq!(inserted.runtime_agent_id, RuntimeAgentIdDto::Debug);
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].runtime_agent_id, RuntimeAgentIdDto::Debug);
        assert_eq!(
            records[0].schema_name.as_deref(),
            Some("xero.project_record.debug_session.v1")
        );
        assert!(records[0].tags.iter().any(|tag| tag == "root-cause"));
    }
}
