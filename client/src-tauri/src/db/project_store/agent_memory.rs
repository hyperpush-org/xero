use std::{collections::BTreeSet, path::Path};

use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{auth::now_timestamp, commands::CommandError, db::database_path_for_repo};

use super::agent_core::AgentRunDiagnosticRecord;
use super::agent_embeddings::embedding_for_storage;
use super::agent_memory_lance::{
    self, AgentMemoryListFilterOwned, AgentMemoryRow, AgentMemoryUpdate, ProjectMemoryStore,
};
use super::freshness::{
    capture_source_fingerprints, evaluate_freshness, freshness_update_changed,
    parse_freshness_state, source_fingerprint_paths, source_fingerprint_paths_overlap,
    CapturedSourceFingerprints, FreshnessRefreshSummary, FreshnessState, SourceFingerprintInput,
    SupersessionUpdate,
};
use super::{
    begin_cross_store_outbox_operation, cross_store_outbox_operation_id,
    finish_cross_store_outbox_operation, load_agent_file_changes, open_runtime_database,
    read_project_row, NewCrossStoreOutboxRecord,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMemoryScope {
    Project,
    Session,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMemoryKind {
    ProjectFact,
    UserPreference,
    Decision,
    SessionSummary,
    Troubleshooting,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentMemoryReviewState {
    Candidate,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMemoryRecord {
    pub id: i64,
    pub memory_id: String,
    pub project_id: String,
    pub agent_session_id: Option<String>,
    pub scope: AgentMemoryScope,
    pub kind: AgentMemoryKind,
    pub text: String,
    pub text_hash: String,
    pub review_state: AgentMemoryReviewState,
    pub enabled: bool,
    pub confidence: Option<u8>,
    pub source_run_id: Option<String>,
    pub source_item_ids: Vec<String>,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentMemoryRecord {
    pub memory_id: String,
    pub project_id: String,
    pub agent_session_id: Option<String>,
    pub scope: AgentMemoryScope,
    pub kind: AgentMemoryKind,
    pub text: String,
    pub review_state: AgentMemoryReviewState,
    pub enabled: bool,
    pub confidence: Option<u8>,
    pub source_run_id: Option<String>,
    pub source_item_ids: Vec<String>,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMemoryUpdateRecord {
    pub project_id: String,
    pub memory_id: String,
    pub review_state: Option<AgentMemoryReviewState>,
    pub enabled: Option<bool>,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentMemoryCorrectionResult {
    pub original: AgentMemoryRecord,
    pub corrected: AgentMemoryRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AgentMemoryListFilter<'a> {
    pub agent_session_id: Option<&'a str>,
    pub include_disabled: bool,
    pub include_rejected: bool,
}

pub fn generate_agent_memory_id() -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    format!(
        "memory-{}",
        bytes
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}

pub fn agent_memory_text_hash(text: &str) -> String {
    let normalized = normalize_memory_text(text);
    let mut hasher = Sha256::new();
    hasher.update(normalized.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn insert_agent_memory(
    repo_root: &Path,
    record: &NewAgentMemoryRecord,
) -> Result<AgentMemoryRecord, CommandError> {
    validate_new_agent_memory(record)?;
    let store = open_store_with_project_check(repo_root, &record.project_id)?;
    let text_hash = agent_memory_text_hash(&record.text);
    let embedding = embedding_for_storage(&agent_memory_embedding_text(record))?;
    let source_fingerprints = capture_source_fingerprints(
        repo_root,
        agent_memory_source_inputs(repo_root, record)?,
        &record.created_at,
    )?;
    let freshness = initial_agent_memory_freshness(source_fingerprints, &record.created_at);
    let fact_key = agent_memory_fact_key(&record.scope, &record.kind, &record.text);
    let row = AgentMemoryRow {
        memory_id: record.memory_id.clone(),
        project_id: record.project_id.clone(),
        agent_session_id: record.agent_session_id.clone(),
        scope: record.scope.clone(),
        kind: record.kind.clone(),
        text: record.text.trim().to_string(),
        text_hash,
        review_state: record.review_state.clone(),
        enabled: record.enabled,
        confidence: record.confidence,
        source_run_id: record.source_run_id.clone(),
        source_item_ids: record.source_item_ids.clone(),
        diagnostic: record.diagnostic.clone(),
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
    let outbox_operation_id = cross_store_outbox_operation_id(
        &record.project_id,
        "agent_memory_lance",
        "approved_memory",
        &record.memory_id,
        "insert",
        &row.text_hash,
    );
    let outbox_row_payload = serde_json::to_value(&row).map_err(|error| {
        CommandError::system_fault(
            "agent_memory_outbox_payload_failed",
            format!("Xero could not prepare an agent-memory outbox payload: {error}"),
        )
    })?;
    begin_cross_store_outbox_operation(
        repo_root,
        &NewCrossStoreOutboxRecord {
            operation_id: outbox_operation_id.clone(),
            project_id: record.project_id.clone(),
            store_kind: "agent_memory_lance".into(),
            entity_kind: "approved_memory".into(),
            entity_id: record.memory_id.clone(),
            operation: "insert".into(),
            payload: serde_json::json!({
                "memoryId": &record.memory_id,
                "textHash": &row.text_hash,
                "row": outbox_row_payload,
            }),
            created_at: record.created_at.clone(),
        },
    )?;
    let inserted = match store.insert(row) {
        Ok(inserted) => inserted,
        Err(error) => {
            let _ = finish_cross_store_outbox_operation(
                repo_root,
                &record.project_id,
                &outbox_operation_id,
                "failed",
                Some(serde_json::json!({
                    "code": error.code.clone(),
                    "message": error.message.clone(),
                })),
                &record.created_at,
            );
            return Err(error);
        }
    };
    if inserted.memory_id == record.memory_id {
        if let Err(error) =
            apply_agent_memory_supersession(&store, &inserted, &fact_key, &record.created_at)
        {
            let _ = finish_cross_store_outbox_operation(
                repo_root,
                &record.project_id,
                &outbox_operation_id,
                "failed",
                Some(serde_json::json!({
                    "code": error.code.clone(),
                    "message": error.message.clone(),
                    "phase": "post_insert_supersession",
                })),
                &record.created_at,
            );
            return Err(error);
        }
        let refreshed = match store.get_by_memory_id(&inserted.memory_id) {
            Ok(Some(memory)) => memory,
            Ok(None) => {
                let error = missing_agent_memory_error(&record.project_id, &inserted.memory_id);
                let _ = finish_cross_store_outbox_operation(
                    repo_root,
                    &record.project_id,
                    &outbox_operation_id,
                    "failed",
                    Some(serde_json::json!({
                        "code": error.code.clone(),
                        "message": error.message.clone(),
                        "phase": "post_insert_reload",
                    })),
                    &record.created_at,
                );
                return Err(error);
            }
            Err(error) => {
                let _ = finish_cross_store_outbox_operation(
                    repo_root,
                    &record.project_id,
                    &outbox_operation_id,
                    "failed",
                    Some(serde_json::json!({
                        "code": error.code.clone(),
                        "message": error.message.clone(),
                        "phase": "post_insert_reload",
                    })),
                    &record.created_at,
                );
                return Err(error);
            }
        };
        let _ = finish_cross_store_outbox_operation(
            repo_root,
            &record.project_id,
            &outbox_operation_id,
            "applied",
            Some(serde_json::json!({
                "insertedId": &refreshed.memory_id,
            })),
            &record.created_at,
        );
        return Ok(refreshed);
    }
    let _ = finish_cross_store_outbox_operation(
        repo_root,
        &record.project_id,
        &outbox_operation_id,
        "applied",
        Some(serde_json::json!({
            "insertedId": &inserted.memory_id,
        })),
        &record.created_at,
    );
    Ok(inserted)
}

fn apply_agent_memory_supersession(
    store: &ProjectMemoryStore,
    accepted: &AgentMemoryRecord,
    fact_key: &str,
    now: &str,
) -> Result<(), CommandError> {
    if accepted.review_state != AgentMemoryReviewState::Approved
        || !accepted.enabled
        || parse_freshness_state(&accepted.freshness_state) == FreshnessState::Superseded
    {
        return Ok(());
    }

    let mut superseded_ids = Vec::new();
    for row in store.list_all_rows()? {
        if row.memory_id == accepted.memory_id
            || row.review_state != AgentMemoryReviewState::Approved
            || !row.enabled
            || row.freshness_state != FreshnessState::Current.as_str()
        {
            continue;
        }
        let row_fact_key = row
            .fact_key
            .clone()
            .unwrap_or_else(|| agent_memory_fact_key(&row.scope, &row.kind, &row.text));
        if row_fact_key != fact_key {
            continue;
        }
        if !source_fingerprint_paths_overlap(
            &row.source_fingerprints_json,
            &accepted.source_fingerprints_json,
        )? {
            continue;
        }
        store.update_supersession(
            &row.memory_id,
            SupersessionUpdate {
                superseded_by_id: Some(accepted.memory_id.clone()),
                supersedes_id: row.supersedes_id,
                fact_key: Some(fact_key.to_string()),
                invalidated_at: Some(now.to_string()),
                stale_reason: Some(format!(
                    "Superseded by newer durable memory `{}`.",
                    accepted.memory_id
                )),
                updated_at: now.to_string(),
            },
        )?;
        superseded_ids.push(row.memory_id);
    }

    if let Some(supersedes_id) = superseded_ids.first() {
        store.update_supersession(
            &accepted.memory_id,
            SupersessionUpdate {
                superseded_by_id: None,
                supersedes_id: Some(supersedes_id.clone()),
                fact_key: Some(fact_key.to_string()),
                invalidated_at: accepted.invalidated_at.clone(),
                stale_reason: accepted.stale_reason.clone(),
                updated_at: now.to_string(),
            },
        )?;
    } else if accepted.fact_key.as_deref() != Some(fact_key) {
        store.update_supersession(
            &accepted.memory_id,
            SupersessionUpdate {
                superseded_by_id: None,
                supersedes_id: accepted.supersedes_id.clone(),
                fact_key: Some(fact_key.to_string()),
                invalidated_at: accepted.invalidated_at.clone(),
                stale_reason: accepted.stale_reason.clone(),
                updated_at: now.to_string(),
            },
        )?;
    }

    Ok(())
}

fn agent_memory_fact_key(scope: &AgentMemoryScope, kind: &AgentMemoryKind, text: &str) -> String {
    format!(
        "memory:{}:{}:{}",
        agent_memory_scope_fact_value(scope),
        agent_memory_kind_fact_value(kind),
        agent_memory_short_subject(text)
    )
}

fn agent_memory_short_subject(text: &str) -> String {
    let normalized = normalize_fact_segment(text);
    let source = normalized.as_str();
    for marker in [
        " now ",
        " currently ",
        " lives in ",
        " live in ",
        " lives at ",
        " live at ",
        " derives from ",
        " depends on ",
        " moved to ",
        " is in ",
        " are in ",
        " is ",
        " are ",
    ] {
        if let Some(index) = source.find(marker) {
            let candidate = source[..index].trim();
            if candidate.split_whitespace().count() >= 2 {
                return trim_leading_article(candidate);
            }
        }
    }

    trim_leading_article(
        &source
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join(" "),
    )
}

fn normalize_fact_segment(value: &str) -> String {
    value
        .split_whitespace()
        .map(|segment| segment.trim_matches(|ch: char| ch.is_ascii_punctuation()))
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn trim_leading_article(value: &str) -> String {
    let mut words = value.split_whitespace().collect::<Vec<_>>();
    if words
        .first()
        .is_some_and(|word| matches!(*word, "a" | "an" | "the"))
    {
        words.remove(0);
    }
    if words.is_empty() {
        value.to_string()
    } else {
        words.join(" ")
    }
}

fn agent_memory_scope_fact_value(scope: &AgentMemoryScope) -> &'static str {
    match scope {
        AgentMemoryScope::Project => "project",
        AgentMemoryScope::Session => "session",
    }
}

fn agent_memory_kind_fact_value(kind: &AgentMemoryKind) -> &'static str {
    match kind {
        AgentMemoryKind::ProjectFact => "project_fact",
        AgentMemoryKind::UserPreference => "user_preference",
        AgentMemoryKind::Decision => "decision",
        AgentMemoryKind::SessionSummary => "session_summary",
        AgentMemoryKind::Troubleshooting => "troubleshooting",
    }
}

struct AgentMemoryInitialFreshness {
    freshness_state: FreshnessState,
    freshness_checked_at: Option<String>,
    stale_reason: Option<String>,
    source_fingerprints_json: String,
    invalidated_at: Option<String>,
}

fn agent_memory_source_inputs(
    repo_root: &Path,
    record: &NewAgentMemoryRecord,
) -> Result<Vec<SourceFingerprintInput>, CommandError> {
    let Some(source_run_id) = record.source_run_id.as_deref() else {
        return Ok(Vec::new());
    };
    let selected_file_change_ids = file_change_source_item_ids(&record.source_item_ids);
    let inputs = load_agent_file_changes(repo_root, &record.project_id, source_run_id)?
        .into_iter()
        .filter(|change| {
            selected_file_change_ids.is_empty() || selected_file_change_ids.contains(&change.id)
        })
        .map(|change| {
            SourceFingerprintInput::agent_file_change(
                change.path,
                format!("file_change:{}", change.id),
                change.operation,
            )
        })
        .collect();
    Ok(inputs)
}

fn file_change_source_item_ids(source_item_ids: &[String]) -> BTreeSet<i64> {
    source_item_ids
        .iter()
        .filter_map(|item_id| {
            item_id
                .strip_prefix("file_change:")
                .or_else(|| item_id.strip_prefix("agent_file_changes:"))
                .and_then(|id| id.parse::<i64>().ok())
        })
        .collect()
}

fn initial_agent_memory_freshness(
    source_fingerprints: CapturedSourceFingerprints,
    created_at: &str,
) -> AgentMemoryInitialFreshness {
    let invalidated_at = if source_fingerprints.freshness_state == FreshnessState::SourceMissing {
        Some(created_at.into())
    } else {
        None
    };
    AgentMemoryInitialFreshness {
        freshness_state: source_fingerprints.freshness_state,
        freshness_checked_at: source_fingerprints.freshness_checked_at,
        stale_reason: source_fingerprints.stale_reason,
        source_fingerprints_json: source_fingerprints.source_fingerprints_json,
        invalidated_at,
    }
}

pub fn list_agent_memories(
    repo_root: &Path,
    project_id: &str,
    filter: AgentMemoryListFilter<'_>,
) -> Result<Vec<AgentMemoryRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    if let Some(agent_session_id) = filter.agent_session_id {
        validate_non_empty_text(agent_session_id, "agentSessionId")?;
    }
    let store = open_store_with_project_check(repo_root, project_id)?;
    store.list(
        filter.agent_session_id,
        AgentMemoryListFilterOwned {
            include_disabled: filter.include_disabled,
            include_rejected: filter.include_rejected,
        },
    )
}

pub fn list_approved_agent_memories(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: Option<&str>,
) -> Result<Vec<AgentMemoryRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    if let Some(agent_session_id) = agent_session_id {
        validate_non_empty_text(agent_session_id, "agentSessionId")?;
    }
    let store = open_store_no_project_check(repo_root, project_id);
    Ok(store
        .list_approved(agent_session_id)?
        .into_iter()
        .filter(is_retrievable_agent_memory)
        .collect())
}

pub fn load_agent_memory_review_queue(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: Option<&str>,
    limit: usize,
) -> Result<serde_json::Value, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    if let Some(agent_session_id) = agent_session_id {
        validate_non_empty_text(agent_session_id, "agentSessionId")?;
    }
    let limit = limit.clamp(1, 100);
    let memories = list_agent_memories(
        repo_root,
        project_id,
        AgentMemoryListFilter {
            agent_session_id,
            include_disabled: true,
            include_rejected: true,
        },
    )?;
    let mut candidate_count = 0_usize;
    let mut approved_count = 0_usize;
    let mut rejected_count = 0_usize;
    let mut disabled_count = 0_usize;
    let mut retrievable_approved_count = 0_usize;
    for memory in &memories {
        match memory.review_state {
            AgentMemoryReviewState::Candidate => candidate_count += 1,
            AgentMemoryReviewState::Approved => approved_count += 1,
            AgentMemoryReviewState::Rejected => rejected_count += 1,
        }
        if !memory.enabled {
            disabled_count += 1;
        }
        if is_retrievable_agent_memory(memory) {
            retrievable_approved_count += 1;
        }
    }
    let items = memories
        .iter()
        .take(limit)
        .map(memory_review_queue_item)
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "schema": "xero.agent_memory_review_queue.v1",
        "projectId": project_id,
        "agentSessionId": agent_session_id,
        "limit": limit,
        "counts": {
            "candidate": candidate_count,
            "approved": approved_count,
            "rejected": rejected_count,
            "disabled": disabled_count,
            "retrievableApproved": retrievable_approved_count
        },
        "items": items,
        "actions": {
            "approve": "Set reviewState to approved; enabled memories become retrievable when redaction and freshness allow it.",
            "reject": "Set reviewState to rejected and disabled so retrieval excludes it.",
            "disable": "Keep the record for provenance but exclude it from retrieval.",
            "delete": "Remove the memory record from the approved-memory retrieval store.",
            "edit": "Create a corrected memory or superseding project record; direct text mutation is intentionally not part of this backend contract."
        },
        "uiDeferred": true
    }))
}

pub fn find_active_agent_memory_by_hash(
    repo_root: &Path,
    project_id: &str,
    scope: &AgentMemoryScope,
    agent_session_id: Option<&str>,
    kind: &AgentMemoryKind,
    text_hash: &str,
) -> Result<Option<AgentMemoryRecord>, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_sha256(text_hash, "textHash")?;
    let store = open_store_no_project_check(repo_root, project_id);
    store.find_active_by_hash(scope, agent_session_id, kind, text_hash)
}

pub fn update_agent_memory(
    repo_root: &Path,
    update: &AgentMemoryUpdateRecord,
) -> Result<AgentMemoryRecord, CommandError> {
    validate_non_empty_text(&update.project_id, "projectId")?;
    validate_non_empty_text(&update.memory_id, "memoryId")?;
    if update.review_state.is_none() && update.enabled.is_none() && update.diagnostic.is_none() {
        return Err(CommandError::invalid_request("memoryUpdate"));
    }

    let store = open_store_with_project_check(repo_root, &update.project_id)?;
    let updated = store.update(
        AgentMemoryUpdate {
            project_id: update.project_id.clone(),
            memory_id: update.memory_id.clone(),
            review_state: update.review_state.clone(),
            enabled: update.enabled,
            diagnostic: update.diagnostic.clone(),
        },
        now_timestamp(),
    )?;
    if updated.review_state == AgentMemoryReviewState::Approved && updated.enabled {
        let fact_key = updated
            .fact_key
            .clone()
            .unwrap_or_else(|| agent_memory_fact_key(&updated.scope, &updated.kind, &updated.text));
        apply_agent_memory_supersession(&store, &updated, &fact_key, &updated.updated_at)?;
        return store
            .get_by_memory_id(&updated.memory_id)?
            .ok_or_else(|| missing_agent_memory_error(&update.project_id, &updated.memory_id));
    }
    Ok(updated)
}

pub fn correct_agent_memory(
    repo_root: &Path,
    project_id: &str,
    memory_id: &str,
    corrected_text: &str,
    now: &str,
) -> Result<AgentMemoryCorrectionResult, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(memory_id, "memoryId")?;
    validate_non_empty_text(corrected_text, "correctedText")?;
    validate_non_empty_text(now, "createdAt")?;

    let original = get_agent_memory(repo_root, project_id, memory_id)?;
    let corrected_id = generate_agent_memory_id();
    let corrected_session_id = match original.scope {
        AgentMemoryScope::Project => None,
        AgentMemoryScope::Session => original.agent_session_id.clone(),
    };
    let mut source_item_ids = original.source_item_ids.clone();
    source_item_ids.push(format!("corrected-memory:{memory_id}"));
    let corrected = insert_agent_memory(
        repo_root,
        &NewAgentMemoryRecord {
            memory_id: corrected_id.clone(),
            project_id: project_id.to_string(),
            agent_session_id: corrected_session_id,
            scope: original.scope.clone(),
            kind: original.kind.clone(),
            text: corrected_text.trim().to_string(),
            review_state: AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: original.confidence,
            source_run_id: original.source_run_id.clone(),
            source_item_ids,
            diagnostic: Some(AgentRunDiagnosticRecord {
                code: "memory_corrected_by_user".into(),
                message: format!("Corrected memory `{memory_id}`."),
            }),
            created_at: now.to_string(),
        },
    )?;
    mark_agent_memory_superseded_by(repo_root, project_id, memory_id, &corrected_id, now)?;
    let original = get_agent_memory(repo_root, project_id, memory_id)?;
    let corrected = get_agent_memory(repo_root, project_id, &corrected.memory_id)?;
    Ok(AgentMemoryCorrectionResult {
        original,
        corrected,
    })
}

pub fn mark_agent_memory_superseded_by(
    repo_root: &Path,
    project_id: &str,
    superseded_memory_id: &str,
    superseding_memory_id: &str,
    now: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(superseded_memory_id, "supersededMemoryId")?;
    validate_non_empty_text(superseding_memory_id, "supersedingMemoryId")?;
    validate_non_empty_text(now, "updatedAt")?;
    if superseded_memory_id == superseding_memory_id {
        return Ok(());
    }

    let store = open_store_with_project_check(repo_root, project_id)?;
    let rows = store.list_all_rows()?;
    let Some(superseded) = rows
        .iter()
        .find(|row| row.memory_id == superseded_memory_id)
    else {
        return Err(CommandError::user_fixable(
            "agent_memory_superseded_not_found",
            format!("Xero could not find memory `{superseded_memory_id}`."),
        ));
    };
    let Some(superseding) = rows
        .iter()
        .find(|row| row.memory_id == superseding_memory_id)
    else {
        return Err(CommandError::user_fixable(
            "agent_memory_superseding_not_found",
            format!("Xero could not find memory `{superseding_memory_id}`."),
        ));
    };
    let fact_key = superseding
        .fact_key
        .clone()
        .or_else(|| superseded.fact_key.clone())
        .unwrap_or_else(|| {
            agent_memory_fact_key(&superseding.scope, &superseding.kind, &superseding.text)
        });
    store.update_supersession(
        superseded_memory_id,
        SupersessionUpdate {
            superseded_by_id: Some(superseding_memory_id.to_string()),
            supersedes_id: superseded.supersedes_id.clone(),
            fact_key: Some(fact_key.clone()),
            invalidated_at: Some(now.to_string()),
            stale_reason: Some(format!(
                "Superseded by corrected durable memory `{superseding_memory_id}`."
            )),
            updated_at: now.to_string(),
        },
    )?;
    store.update_supersession(
        superseding_memory_id,
        SupersessionUpdate {
            superseded_by_id: superseding.superseded_by_id.clone(),
            supersedes_id: Some(superseded_memory_id.to_string()),
            fact_key: Some(fact_key),
            invalidated_at: superseding.invalidated_at.clone(),
            stale_reason: superseding.stale_reason.clone(),
            updated_at: now.to_string(),
        },
    )?;
    Ok(())
}

pub fn get_agent_memory(
    repo_root: &Path,
    project_id: &str,
    memory_id: &str,
) -> Result<AgentMemoryRecord, CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(memory_id, "memoryId")?;
    let store = open_store_no_project_check(repo_root, project_id);
    store
        .get_by_memory_id(memory_id)?
        .ok_or_else(|| missing_agent_memory_error(project_id, memory_id))
}

fn memory_review_queue_item(memory: &AgentMemoryRecord) -> serde_json::Value {
    let (text_preview, text_redacted) = memory_review_text_preview(&memory.text);
    let (fact_key, fact_key_redacted) =
        memory_review_optional_redacted_text(memory.fact_key.as_deref());
    let retrieval_eligible = is_retrievable_agent_memory(memory);
    serde_json::json!({
        "memoryId": memory.memory_id,
        "scope": agent_memory_scope_fact_value(&memory.scope),
        "kind": agent_memory_kind_fact_value(&memory.kind),
        "reviewState": agent_memory_review_state_value(&memory.review_state),
        "enabled": memory.enabled,
        "confidence": memory.confidence,
        "textPreview": text_preview,
        "textHash": memory.text_hash,
        "provenance": {
            "sourceRunId": memory.source_run_id,
            "sourceItemIds": memory.source_item_ids,
            "diagnostic": memory.diagnostic.as_ref().map(|diagnostic| serde_json::json!({
                "code": diagnostic.code,
                "message": diagnostic.message,
            }))
        },
        "freshness": {
            "state": memory.freshness_state,
            "checkedAt": memory.freshness_checked_at,
            "staleReason": memory.stale_reason,
            "supersedesId": memory.supersedes_id,
            "supersededById": memory.superseded_by_id,
            "invalidatedAt": memory.invalidated_at,
            "factKey": fact_key,
        },
        "retrieval": {
            "eligible": retrieval_eligible,
            "reason": agent_memory_retrieval_reason(memory),
        },
        "redaction": {
            "textPreviewRedacted": text_redacted,
            "factKeyRedacted": fact_key_redacted,
            "rawTextHidden": true,
        },
        "availableActions": {
            "canApprove": memory.review_state != AgentMemoryReviewState::Approved && !text_redacted,
            "canReject": memory.review_state != AgentMemoryReviewState::Rejected,
            "canDisable": memory.enabled,
            "canDelete": true,
            "canEditByCorrection": true
        },
        "createdAt": memory.created_at,
        "updatedAt": memory.updated_at,
    })
}

fn memory_review_optional_redacted_text(value: Option<&str>) -> (serde_json::Value, bool) {
    let Some(value) = value else {
        return (serde_json::Value::Null, false);
    };
    let value = serde_json::Value::String(value.to_string());
    let (redacted, was_redacted) = crate::runtime::redaction::redact_json_for_persistence(&value);
    if was_redacted {
        (serde_json::Value::Null, true)
    } else {
        (redacted, false)
    }
}

fn memory_review_text_preview(text: &str) -> (serde_json::Value, bool) {
    let preview = text.chars().take(240).collect::<String>();
    let value = serde_json::Value::String(preview);
    let (redacted, was_redacted) = crate::runtime::redaction::redact_json_for_persistence(&value);
    if was_redacted {
        (serde_json::Value::Null, true)
    } else {
        (redacted, false)
    }
}

pub fn is_retrievable_agent_memory(memory: &AgentMemoryRecord) -> bool {
    agent_memory_retrieval_reason(memory) == "retrievable"
}

pub fn agent_memory_retrieval_reason(memory: &AgentMemoryRecord) -> &'static str {
    agent_memory_retrieval_reason_from_parts(
        &memory.review_state,
        memory.enabled,
        &memory.freshness_state,
        memory.superseded_by_id.as_deref(),
        memory.invalidated_at.as_deref(),
    )
}

pub(crate) fn agent_memory_retrieval_reason_from_parts(
    review_state: &AgentMemoryReviewState,
    enabled: bool,
    freshness_state: &str,
    superseded_by_id: Option<&str>,
    invalidated_at: Option<&str>,
) -> &'static str {
    if *review_state != AgentMemoryReviewState::Approved {
        return "pending_or_rejected_review";
    }
    if !enabled {
        return "disabled";
    }
    if superseded_by_id.is_some() {
        return "superseded";
    }
    match parse_freshness_state(freshness_state) {
        FreshnessState::Current | FreshnessState::SourceUnknown => {}
        FreshnessState::Stale => return "stale",
        FreshnessState::SourceMissing => return "source_missing",
        FreshnessState::Superseded => return "superseded",
        FreshnessState::Blocked => return "blocked",
    }
    if invalidated_at.is_some() {
        return "invalidated";
    }
    "retrievable"
}

fn agent_memory_review_state_value(state: &AgentMemoryReviewState) -> &'static str {
    match state {
        AgentMemoryReviewState::Candidate => "candidate",
        AgentMemoryReviewState::Approved => "approved",
        AgentMemoryReviewState::Rejected => "rejected",
    }
}

pub fn refresh_all_agent_memory_freshness(
    repo_root: &Path,
    project_id: &str,
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let store = open_store_with_project_check(repo_root, project_id)?;
    refresh_agent_memory_rows(repo_root, &store, None, checked_at)
}

pub fn refresh_agent_memory_freshness_for_paths(
    repo_root: &Path,
    project_id: &str,
    paths: &[String],
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let store = open_store_with_project_check(repo_root, project_id)?;
    let paths = paths.iter().cloned().collect::<BTreeSet<_>>();
    refresh_agent_memory_rows(repo_root, &store, Some(&paths), checked_at)
}

pub fn refresh_agent_memory_freshness_for_ids(
    repo_root: &Path,
    project_id: &str,
    memory_ids: &[String],
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let store = open_store_with_project_check(repo_root, project_id)?;
    let memory_ids = memory_ids.iter().cloned().collect::<BTreeSet<_>>();
    refresh_agent_memory_rows_for_ids(repo_root, &store, &memory_ids, checked_at)
}

pub(crate) fn refresh_agent_memory_rows(
    repo_root: &Path,
    store: &ProjectMemoryStore,
    paths: Option<&BTreeSet<String>>,
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let mut summary = FreshnessRefreshSummary::default();
    for row in store.list_all_rows()? {
        if !memory_matches_freshness_paths(&row.source_fingerprints_json, paths)? {
            continue;
        }
        let update = evaluate_freshness(
            repo_root,
            parse_freshness_state(&row.freshness_state),
            row.invalidated_at.as_deref(),
            &row.source_fingerprints_json,
            checked_at,
            false,
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
            let _ = store.update_freshness(&row.memory_id, update)?;
        }
    }
    Ok(summary)
}

fn refresh_agent_memory_rows_for_ids(
    repo_root: &Path,
    store: &ProjectMemoryStore,
    memory_ids: &BTreeSet<String>,
    checked_at: &str,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let mut summary = FreshnessRefreshSummary::default();
    if memory_ids.is_empty() {
        return Ok(summary);
    }
    for row in store.list_all_rows()? {
        if !memory_ids.contains(&row.memory_id) {
            continue;
        }
        let update = evaluate_freshness(
            repo_root,
            parse_freshness_state(&row.freshness_state),
            row.invalidated_at.as_deref(),
            &row.source_fingerprints_json,
            checked_at,
            false,
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
            let _ = store.update_freshness(&row.memory_id, update)?;
        }
    }
    Ok(summary)
}

fn memory_matches_freshness_paths(
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

pub fn delete_agent_memory(
    repo_root: &Path,
    project_id: &str,
    memory_id: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(project_id, "projectId")?;
    validate_non_empty_text(memory_id, "memoryId")?;
    let store = open_store_with_project_check(repo_root, project_id)?;
    let removed = store.delete(memory_id)?;
    if !removed {
        return Err(missing_agent_memory_error(project_id, memory_id));
    }
    Ok(())
}

/// Clear Lance provenance references for runs deleted by the relational
/// runtime store.
pub fn clear_memory_runs_for_deletion(
    repo_root: &Path,
    project_id: &str,
    run_ids: &[String],
) -> Result<usize, CommandError> {
    if run_ids.is_empty() {
        return Ok(0);
    }
    let store = open_store_no_project_check(repo_root, project_id);
    store.clear_runs(run_ids, &now_timestamp())
}

fn open_store_with_project_check(
    repo_root: &Path,
    project_id: &str,
) -> Result<ProjectMemoryStore, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    drop(connection);
    Ok(agent_memory_lance::open_for_database_path(
        &database_path,
        project_id,
    ))
}

fn open_store_no_project_check(repo_root: &Path, project_id: &str) -> ProjectMemoryStore {
    let database_path = database_path_for_repo(repo_root);
    agent_memory_lance::open_for_database_path(&database_path, project_id)
}

fn validate_new_agent_memory(record: &NewAgentMemoryRecord) -> Result<(), CommandError> {
    validate_non_empty_text(&record.memory_id, "memoryId")?;
    validate_non_empty_text(&record.project_id, "projectId")?;
    validate_non_empty_text(&record.text, "text")?;
    validate_non_empty_text(&record.created_at, "createdAt")?;
    match record.scope {
        AgentMemoryScope::Project if record.agent_session_id.is_some() => {
            return Err(CommandError::invalid_request("agentSessionId"));
        }
        AgentMemoryScope::Session => {
            validate_non_empty_text(
                record.agent_session_id.as_deref().unwrap_or_default(),
                "agentSessionId",
            )?;
        }
        AgentMemoryScope::Project => {}
    }
    if record.enabled && record.review_state != AgentMemoryReviewState::Approved {
        return Err(CommandError::invalid_request("enabled"));
    }
    if record
        .source_item_ids
        .iter()
        .any(|item_id| item_id.trim().is_empty())
    {
        return Err(CommandError::invalid_request("sourceItemIds"));
    }
    if let Some(source_run_id) = record.source_run_id.as_deref() {
        validate_non_empty_text(source_run_id, "sourceRunId")?;
    }
    Ok(())
}

fn normalize_memory_text(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn agent_memory_embedding_text(record: &NewAgentMemoryRecord) -> String {
    format!(
        "{:?} {:?}\n{}",
        record.scope,
        record.kind,
        record.text.trim()
    )
}

fn validate_sha256(value: &str, field: &'static str) -> Result<(), CommandError> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        Ok(())
    } else {
        Err(CommandError::invalid_request(field))
    }
}

fn validate_non_empty_text(value: &str, field: &'static str) -> Result<(), CommandError> {
    if value.trim().is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(())
}

fn missing_agent_memory_error(project_id: &str, memory_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_memory_not_found",
        format!("Xero could not find memory `{memory_id}` for project `{project_id}`."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;

    use rusqlite::{params, Connection};
    use serde_json::Value as JsonValue;

    use crate::db::project_store::{
        append_agent_file_change, create_agent_session, insert_agent_run, search_agent_context,
        AgentContextRetrievalFilters, AgentContextRetrievalRequest, AgentRetrievalSearchScope,
        AgentSessionCreateRecord, AgentSessionRecord, NewAgentFileChangeRecord, NewAgentRunRecord,
        BUILTIN_AGENT_DEFINITION_VERSION,
    };
    use crate::{
        commands::RuntimeAgentIdDto,
        db::{configure_connection, migrations::migrations},
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
        crate::db::register_project_database_path(repo_root, &database_path);
        database_path
    }

    fn seed_agent_run(repo_root: &Path, project_id: &str, run_id: &str) -> AgentSessionRecord {
        let session = create_agent_session(
            repo_root,
            &AgentSessionCreateRecord {
                project_id: project_id.into(),
                title: "Memory source run".into(),
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
                prompt: "Remember this source file.".into(),
                system_prompt: "system".into(),
                now: "2026-05-03T00:00:00Z".into(),
            },
        )
        .expect("insert agent run");
        session
    }

    #[test]
    fn agent_memory_text_hash_is_normalized() {
        let a = agent_memory_text_hash(" Hello   World ");
        let b = agent_memory_text_hash("hello world");
        assert_eq!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn validate_new_rejects_session_scope_without_session_id() {
        let record = NewAgentMemoryRecord {
            memory_id: "memory-x".into(),
            project_id: "project-x".into(),
            agent_session_id: None,
            scope: AgentMemoryScope::Session,
            kind: AgentMemoryKind::Decision,
            text: "Body".into(),
            review_state: AgentMemoryReviewState::Candidate,
            enabled: false,
            confidence: None,
            source_run_id: None,
            source_item_ids: vec![],
            diagnostic: None,
            created_at: "2026-04-26T00:00:00Z".into(),
        };
        let err = validate_new_agent_memory(&record).expect_err("session needs session id");
        assert_eq!(err.code, "invalid_request");
        assert!(err.message.contains("agentSessionId"));
    }

    #[test]
    fn validate_new_rejects_enabled_unless_approved() {
        let record = NewAgentMemoryRecord {
            memory_id: "memory-x".into(),
            project_id: "project-x".into(),
            agent_session_id: None,
            scope: AgentMemoryScope::Project,
            kind: AgentMemoryKind::Decision,
            text: "Body".into(),
            review_state: AgentMemoryReviewState::Candidate,
            enabled: true,
            confidence: None,
            source_run_id: None,
            source_item_ids: vec![],
            diagnostic: None,
            created_at: "2026-04-26T00:00:00Z".into(),
        };
        let err = validate_new_agent_memory(&record).expect_err("enabled requires approved");
        assert_eq!(err.code, "invalid_request");
        assert!(err.message.contains("enabled"));
    }

    #[test]
    fn retrieval_predicate_rejects_non_approved_or_contradicted_memory() {
        let mut memory = AgentMemoryRecord {
            id: 1,
            memory_id: "memory-retrieval-contract".into(),
            project_id: "project-retrieval-contract".into(),
            agent_session_id: None,
            scope: AgentMemoryScope::Project,
            kind: AgentMemoryKind::ProjectFact,
            text: "Durable memory retrieval predicate fixture.".into(),
            text_hash: "a".repeat(64),
            review_state: AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: Some(90),
            source_run_id: Some("run-retrieval-contract".into()),
            source_item_ids: vec!["agent_messages:1".into()],
            diagnostic: Some(AgentRunDiagnosticRecord {
                code: "memory_promotion_gate_promoted".into(),
                message: "{}".into(),
            }),
            freshness_state: FreshnessState::Current.as_str().into(),
            freshness_checked_at: None,
            stale_reason: None,
            source_fingerprints_json: crate::db::project_store::source_fingerprints_empty_json(),
            supersedes_id: None,
            superseded_by_id: None,
            invalidated_at: None,
            fact_key: None,
            created_at: "2026-05-09T00:00:00Z".into(),
            updated_at: "2026-05-09T00:00:00Z".into(),
        };
        assert!(is_retrievable_agent_memory(&memory));

        memory.review_state = AgentMemoryReviewState::Candidate;
        assert_eq!(
            agent_memory_retrieval_reason(&memory),
            "pending_or_rejected_review"
        );

        memory.review_state = AgentMemoryReviewState::Approved;
        memory.enabled = false;
        assert_eq!(agent_memory_retrieval_reason(&memory), "disabled");

        memory.enabled = true;
        memory.freshness_state = FreshnessState::Stale.as_str().into();
        assert_eq!(agent_memory_retrieval_reason(&memory), "stale");

        memory.freshness_state = FreshnessState::Current.as_str().into();
        memory.superseded_by_id = Some("memory-newer".into());
        assert_eq!(agent_memory_retrieval_reason(&memory), "superseded");
    }

    #[test]
    fn s28_memory_review_delete_removes_approved_memory_from_retrieval() {
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-memory-review-delete";
        create_project_database(&repo_root, project_id);

        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-delete-review".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "Memory review delete should remove this approved fact from retrieval."
                    .into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(90),
                source_run_id: None,
                source_item_ids: Vec::new(),
                diagnostic: None,
                created_at: "2026-05-09T00:00:00Z".into(),
            },
        )
        .expect("insert approved memory");
        assert!(list_approved_agent_memories(&repo_root, project_id, None)
            .expect("list approved before delete")
            .iter()
            .any(|memory| memory.memory_id == "memory-delete-review"));

        delete_agent_memory(&repo_root, project_id, "memory-delete-review")
            .expect("delete approved memory");

        assert!(!list_approved_agent_memories(&repo_root, project_id, None)
            .expect("list approved after delete")
            .iter()
            .any(|memory| memory.memory_id == "memory-delete-review"));
        assert!(get_agent_memory(&repo_root, project_id, "memory-delete-review").is_err());
    }

    #[test]
    fn s28_memory_review_queue_exposes_actions_provenance_and_retrieval_status() {
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("tempdir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(&repo_root).expect("repo dir");
        let project_id = "project-memory-review-queue";
        create_project_database(&repo_root, project_id);
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-review-candidate".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::Decision,
                text: "Candidate memory awaiting user review.".into(),
                review_state: AgentMemoryReviewState::Candidate,
                enabled: false,
                confidence: Some(82),
                source_run_id: Some("run-memory-review-source".into()),
                source_item_ids: vec!["message:7".into()],
                diagnostic: None,
                created_at: "2026-05-09T00:00:00Z".into(),
            },
        )
        .expect("insert candidate memory");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-review-approved".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "Approved memory should be retrieval eligible.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(95),
                source_run_id: None,
                source_item_ids: Vec::new(),
                diagnostic: None,
                created_at: "2026-05-09T00:01:00Z".into(),
            },
        )
        .expect("insert approved memory");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-review-secret".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::Troubleshooting,
                text: "api_key=sk-s28-memory-review-secret-value".into(),
                review_state: AgentMemoryReviewState::Candidate,
                enabled: false,
                confidence: Some(90),
                source_run_id: Some("run-memory-review-source".into()),
                source_item_ids: vec!["message:secret".into()],
                diagnostic: None,
                created_at: "2026-05-09T00:02:00Z".into(),
            },
        )
        .expect("insert redaction candidate memory");

        let queue = load_agent_memory_review_queue(&repo_root, project_id, None, 10)
            .expect("load memory review queue");

        assert_eq!(
            queue["schema"],
            serde_json::json!("xero.agent_memory_review_queue.v1")
        );
        assert_eq!(queue["counts"]["candidate"], serde_json::json!(2));
        assert_eq!(queue["counts"]["approved"], serde_json::json!(1));
        assert_eq!(queue["counts"]["retrievableApproved"], serde_json::json!(1));
        let limited_queue = load_agent_memory_review_queue(&repo_root, project_id, None, 1)
            .expect("load limited review queue");
        assert_eq!(limited_queue["counts"]["candidate"], serde_json::json!(2));
        assert_eq!(
            limited_queue["items"]
                .as_array()
                .expect("limited items")
                .len(),
            1
        );
        let items = queue["items"].as_array().expect("review items");
        let approved = items
            .iter()
            .find(|item| item["memoryId"] == serde_json::json!("memory-review-approved"))
            .expect("approved item");
        assert_eq!(
            approved["retrieval"]["reason"],
            serde_json::json!("retrievable")
        );
        assert_eq!(
            approved["availableActions"]["canDisable"],
            serde_json::json!(true)
        );
        let candidate = items
            .iter()
            .find(|item| item["memoryId"] == serde_json::json!("memory-review-candidate"))
            .expect("candidate item");
        assert_eq!(
            candidate["provenance"]["sourceItemIds"][0],
            serde_json::json!("message:7")
        );
        assert_eq!(
            candidate["retrieval"]["reason"],
            serde_json::json!("pending_or_rejected_review")
        );
        let secret = items
            .iter()
            .find(|item| item["memoryId"] == serde_json::json!("memory-review-secret"))
            .expect("secret item");
        assert!(secret["textPreview"].is_null());
        assert_eq!(
            secret["redaction"]["textPreviewRedacted"],
            serde_json::json!(true)
        );
        assert_eq!(
            secret["availableActions"]["canApprove"],
            serde_json::json!(false)
        );
        let serialized = serde_json::to_string(&queue).expect("serialize queue");
        assert!(!serialized.contains("sk-s28-memory-review-secret-value"));
    }

    #[test]
    fn validate_new_rejects_blank_source_item_ids() {
        let record = NewAgentMemoryRecord {
            memory_id: "memory-x".into(),
            project_id: "project-x".into(),
            agent_session_id: None,
            scope: AgentMemoryScope::Project,
            kind: AgentMemoryKind::Decision,
            text: "Body".into(),
            review_state: AgentMemoryReviewState::Candidate,
            enabled: false,
            confidence: None,
            source_run_id: None,
            source_item_ids: vec!["  ".into()],
            diagnostic: None,
            created_at: "2026-04-26T00:00:00Z".into(),
        };
        let err =
            validate_new_agent_memory(&record).expect_err("blank source ids must be rejected");
        assert_eq!(err.code, "invalid_request");
        assert!(err.message.contains("sourceItemIds"));
    }

    #[test]
    fn validate_sha256_rejects_non_hex() {
        assert!(validate_sha256("ZZZ", "textHash").is_err());
        let valid = "0".repeat(64);
        assert!(validate_sha256(&valid, "textHash").is_ok());
    }

    #[test]
    fn insert_agent_memory_derives_fingerprints_from_source_run_file_changes() {
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(repo_root.join("src/lib.rs"), "pub fn feature() {}\n").expect("write source");
        let project_id = "project-memory-freshness";
        create_project_database(&repo_root, project_id);
        let session = seed_agent_run(&repo_root, project_id, "run-memory-source");
        let file_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-memory-source".into(),
                change_group_id: None,
                path: "src/lib.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:01:00Z".into(),
            },
        )
        .expect("append file change");

        let inserted = insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-source-file".into(),
                project_id: project_id.into(),
                agent_session_id: Some(session.agent_session_id),
                scope: AgentMemoryScope::Session,
                kind: AgentMemoryKind::ProjectFact,
                text: "The feature helper lives in src/lib.rs.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(92),
                source_run_id: Some("run-memory-source".into()),
                source_item_ids: vec![format!("file_change:{}", file_change.id)],
                diagnostic: None,
                created_at: "2026-05-03T00:02:00Z".into(),
            },
        )
        .expect("insert memory");
        let fingerprints: JsonValue =
            serde_json::from_str(&inserted.source_fingerprints_json).expect("fingerprints json");

        assert_eq!(inserted.freshness_state, FreshnessState::Current.as_str());
        assert_eq!(fingerprints["fingerprints"][0]["path"], "src/lib.rs");
        assert_eq!(
            fingerprints["fingerprints"][0]["source"],
            "agent_file_change"
        );
        assert_eq!(
            fingerprints["fingerprints"][0]["sourceItemId"],
            format!("file_change:{}", file_change.id)
        );
        assert!(fingerprints["fingerprints"][0]["hash"].is_string());
        let applied_outbox = crate::db::project_store::list_cross_store_outbox_by_status(
            &repo_root, project_id, "applied",
        )
        .expect("list applied memory outbox");
        assert_eq!(applied_outbox.len(), 1);
        let memory_outbox = &applied_outbox[0];
        assert_eq!(memory_outbox.store_kind, "agent_memory_lance");
        assert_eq!(memory_outbox.entity_kind, "approved_memory");
        assert_eq!(memory_outbox.entity_id, "memory-source-file");
        assert_eq!(memory_outbox.operation, "insert");
        assert_eq!(
            memory_outbox.payload["memoryId"].as_str(),
            Some("memory-source-file")
        );
        assert_eq!(
            memory_outbox.payload["row"]["review_state"].as_str(),
            Some("approved")
        );
        assert_eq!(
            memory_outbox
                .diagnostic
                .as_ref()
                .expect("memory outbox diagnostic")["insertedId"]
                .as_str(),
            Some(inserted.memory_id.as_str())
        );
    }

    #[test]
    fn s29_memory_freshness_invalidates_sources_and_deprioritizes_stale_results() {
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(
            repo_root.join("src/stale_memory.rs"),
            "pub fn memory_rule() -> &'static str { \"old\" }\n",
        )
        .expect("write stale source");
        fs::write(
            repo_root.join("src/current_memory.rs"),
            "pub fn memory_rule() -> &'static str { \"current\" }\n",
        )
        .expect("write current source");
        let project_id = "project-memory-s29-freshness";
        create_project_database(&repo_root, project_id);

        seed_agent_run(&repo_root, project_id, "run-memory-s29-stale");
        let stale_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-memory-s29-stale".into(),
                change_group_id: None,
                path: "src/stale_memory.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T01:01:00Z".into(),
            },
        )
        .expect("append stale file change");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-s29-stale".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "The s29 freshness contract lives in the stale memory source.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(97),
                source_run_id: Some("run-memory-s29-stale".into()),
                source_item_ids: vec![format!("file_change:{}", stale_change.id)],
                diagnostic: None,
                created_at: "2026-05-03T01:02:00Z".into(),
            },
        )
        .expect("insert stale candidate memory");

        seed_agent_run(&repo_root, project_id, "run-memory-s29-current");
        let current_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-memory-s29-current".into(),
                change_group_id: None,
                path: "src/current_memory.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T01:03:00Z".into(),
            },
        )
        .expect("append current file change");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-s29-current".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "The s29 freshness contract lives in the current memory source.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(80),
                source_run_id: Some("run-memory-s29-current".into()),
                source_item_ids: vec![format!("file_change:{}", current_change.id)],
                diagnostic: None,
                created_at: "2026-05-03T01:04:00Z".into(),
            },
        )
        .expect("insert current memory");

        fs::write(
            repo_root.join("src/stale_memory.rs"),
            "pub fn memory_rule() -> &'static str { \"changed\" }\n",
        )
        .expect("mutate stale source");
        let refresh = refresh_agent_memory_freshness_for_paths(
            &repo_root,
            project_id,
            &["src/stale_memory.rs".into()],
            "2026-05-03T01:05:00Z",
        )
        .expect("refresh memory freshness");
        assert_eq!(refresh.inspected_count, 1);
        assert_eq!(refresh.updated_count, 1);
        assert_eq!(refresh.stale_count, 1);

        let memories =
            list_agent_memories(&repo_root, project_id, AgentMemoryListFilter::default())
                .expect("list memories");
        let stale = memories
            .iter()
            .find(|memory| memory.memory_id == "memory-s29-stale")
            .expect("stale memory");
        assert_eq!(stale.freshness_state, FreshnessState::Stale.as_str());
        assert_eq!(
            stale.invalidated_at.as_deref(),
            Some("2026-05-03T01:05:00Z")
        );
        assert!(stale
            .stale_reason
            .as_deref()
            .expect("stale reason")
            .contains("src/stale_memory.rs"));

        let response = search_agent_context(
            &repo_root,
            AgentContextRetrievalRequest {
                query_id: "s29-memory-freshness-query".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                run_id: Some("run-memory-s29-current".into()),
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: RuntimeAgentIdDto::Engineer.as_str().into(),
                agent_definition_version: BUILTIN_AGENT_DEFINITION_VERSION,
                query_text: "s29 freshness contract memory source".into(),
                search_scope: AgentRetrievalSearchScope::ApprovedMemory,
                filters: AgentContextRetrievalFilters::default(),
                limit_count: 2,
                allow_keyword_fallback: true,
                created_at: "2026-05-03T01:06:00Z".into(),
            },
        )
        .expect("search approved memory");

        assert_eq!(
            response
                .results
                .first()
                .expect("current memory result")
                .source_id,
            "memory-s29-current"
        );
        assert!(!response
            .results
            .iter()
            .any(|result| result.source_id == "memory-s29-stale"));
        assert!(response.diagnostic.is_some());
        let exclusions = response.diagnostic.as_ref().unwrap()["freshnessDiagnostics"]
            ["defaultEligibilityExclusionCounts"]
            .as_array()
            .expect("eligibility exclusion counts");
        assert!(exclusions.iter().any(|entry| {
            entry["reason"] == serde_json::json!("stale")
                && entry["count"].as_u64().unwrap_or_default() >= 1
        }));
    }

    #[test]
    fn s41_agent_memory_outbox_reconciles_existing_and_replays_missing_rows() {
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(repo_root.join("src/lib.rs"), "pub fn feature() {}\n").expect("write source");
        let project_id = "project-memory-outbox-reconcile";
        create_project_database(&repo_root, project_id);

        let session = seed_agent_run(&repo_root, project_id, "run-memory-outbox-existing");
        let existing = insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-outbox-existing".into(),
                project_id: project_id.into(),
                agent_session_id: Some(session.agent_session_id),
                scope: AgentMemoryScope::Session,
                kind: AgentMemoryKind::ProjectFact,
                text: "The outbox existing memory is already in Lance.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(90),
                source_run_id: Some("run-memory-outbox-existing".into()),
                source_item_ids: Vec::new(),
                diagnostic: None,
                created_at: "2026-05-03T00:10:00Z".into(),
            },
        )
        .expect("insert existing memory");

        crate::db::project_store::begin_cross_store_outbox_operation(
            &repo_root,
            &crate::db::project_store::NewCrossStoreOutboxRecord {
                operation_id: "cross-store-reconcile-existing-memory".into(),
                project_id: project_id.into(),
                store_kind: "agent_memory_lance".into(),
                entity_kind: "approved_memory".into(),
                entity_id: existing.memory_id.clone(),
                operation: "insert".into(),
                payload: serde_json::json!({
                    "memoryId": &existing.memory_id,
                    "source": "test-existing-memory-reconciliation",
                }),
                created_at: "2026-05-03T00:11:00Z".into(),
            },
        )
        .expect("seed existing-memory pending outbox");

        let replay_text = "The outbox can replay missing approved memory rows.";
        let replay_text_hash = agent_memory_text_hash(replay_text);
        let replay_row = AgentMemoryRow {
            memory_id: "memory-outbox-replayed".into(),
            project_id: project_id.into(),
            agent_session_id: None,
            scope: AgentMemoryScope::Project,
            kind: AgentMemoryKind::ProjectFact,
            text: replay_text.into(),
            text_hash: replay_text_hash.clone(),
            review_state: AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: Some(88),
            source_run_id: None,
            source_item_ids: Vec::new(),
            diagnostic: None,
            freshness_state: FreshnessState::Current.as_str().into(),
            freshness_checked_at: Some("2026-05-03T00:12:00Z".into()),
            stale_reason: None,
            source_fingerprints_json: serde_json::json!({"fingerprints": []}).to_string(),
            supersedes_id: None,
            superseded_by_id: None,
            invalidated_at: None,
            fact_key: Some(agent_memory_fact_key(
                &AgentMemoryScope::Project,
                &AgentMemoryKind::ProjectFact,
                replay_text,
            )),
            created_at: "2026-05-03T00:12:00Z".into(),
            updated_at: "2026-05-03T00:12:00Z".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimension: None,
            embedding_version: None,
        };
        let replay_operation_id = crate::db::project_store::cross_store_outbox_operation_id(
            project_id,
            "agent_memory_lance",
            "approved_memory",
            &replay_row.memory_id,
            "insert",
            &replay_text_hash,
        );
        crate::db::project_store::begin_cross_store_outbox_operation(
            &repo_root,
            &crate::db::project_store::NewCrossStoreOutboxRecord {
                operation_id: replay_operation_id.clone(),
                project_id: project_id.into(),
                store_kind: "agent_memory_lance".into(),
                entity_kind: "approved_memory".into(),
                entity_id: replay_row.memory_id.clone(),
                operation: "insert".into(),
                payload: serde_json::json!({
                    "memoryId": &replay_row.memory_id,
                    "textHash": &replay_row.text_hash,
                    "row": serde_json::to_value(&replay_row).expect("serialize replay row"),
                }),
                created_at: "2026-05-03T00:12:00Z".into(),
            },
        )
        .expect("seed replayable memory outbox");
        crate::db::project_store::finish_cross_store_outbox_operation(
            &repo_root,
            project_id,
            &replay_operation_id,
            "failed",
            Some(serde_json::json!({
                "code": "simulated_lance_write_interruption",
                "message": "The Lance insert did not complete before shutdown.",
            })),
            "2026-05-03T00:13:00Z",
        )
        .expect("mark replayable memory outbox failed");

        let report = crate::db::project_store::reconcile_cross_store_outbox(
            &repo_root,
            project_id,
            "2026-05-03T00:14:00Z",
        )
        .expect("reconcile memory outbox");

        assert_eq!(report.inspected_count, 2);
        assert_eq!(report.reconciled_count, 2);
        assert_eq!(report.failed_count, 0);
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.operation_id == "cross-store-reconcile-existing-memory"
                && diagnostic.status_before == "pending"
                && diagnostic.status_after == "reconciled"
        }));
        assert!(report.diagnostics.iter().any(|diagnostic| {
            diagnostic.operation_id == replay_operation_id
                && diagnostic.status_before == "failed"
                && diagnostic.status_after == "reconciled"
        }));

        let project_memories =
            list_agent_memories(&repo_root, project_id, AgentMemoryListFilter::default())
                .expect("list reconciled project memories");
        let existing_session_memories = list_agent_memories(
            &repo_root,
            project_id,
            AgentMemoryListFilter {
                agent_session_id: existing.agent_session_id.as_deref(),
                ..AgentMemoryListFilter::default()
            },
        )
        .expect("list reconciled session memories");
        assert_eq!(
            existing_session_memories
                .iter()
                .filter(|memory| memory.memory_id == "memory-outbox-existing")
                .count(),
            1
        );
        assert_eq!(
            project_memories
                .iter()
                .filter(|memory| memory.memory_id == "memory-outbox-replayed")
                .count(),
            1
        );
        let second_report = crate::db::project_store::reconcile_cross_store_outbox(
            &repo_root,
            project_id,
            "2026-05-03T00:15:00Z",
        )
        .expect("reconcile memory outbox again");
        assert_eq!(second_report.inspected_count, 0);
    }

    #[test]
    fn approved_agent_memory_supersedes_current_memory_with_same_fact_key() {
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(
            repo_root.join("src/memory_subject.rs"),
            "pub fn subject() {}\n",
        )
        .expect("write source");
        let project_id = "project-memory-supersession";
        create_project_database(&repo_root, project_id);
        seed_agent_run(&repo_root, project_id, "run-memory-supersession-old");
        let old_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-memory-supersession-old".into(),
                change_group_id: None,
                path: "src/memory_subject.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:01:00Z".into(),
            },
        )
        .expect("append old file change");
        let old = insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-supersession-old".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "The freshness memory subject lives in the legacy cache.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(90),
                source_run_id: Some("run-memory-supersession-old".into()),
                source_item_ids: vec![format!("file_change:{}", old_change.id)],
                diagnostic: None,
                created_at: "2026-05-03T00:02:00Z".into(),
            },
        )
        .expect("insert old memory");

        seed_agent_run(&repo_root, project_id, "run-memory-supersession-new");
        let new_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-memory-supersession-new".into(),
                change_group_id: None,
                path: "src/memory_subject.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:03:00Z".into(),
            },
        )
        .expect("append new file change");
        let new = insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-supersession-new".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "The freshness memory subject now lives in the durable cache.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(95),
                source_run_id: Some("run-memory-supersession-new".into()),
                source_item_ids: vec![format!("file_change:{}", new_change.id)],
                diagnostic: None,
                created_at: "2026-05-03T00:04:00Z".into(),
            },
        )
        .expect("insert new memory");
        let memories =
            list_agent_memories(&repo_root, project_id, AgentMemoryListFilter::default())
                .expect("list memories");
        let old = memories
            .iter()
            .find(|memory| memory.memory_id == old.memory_id)
            .expect("old memory");
        let new = memories
            .iter()
            .find(|memory| memory.memory_id == new.memory_id)
            .expect("new memory");

        assert_eq!(old.freshness_state, FreshnessState::Superseded.as_str());
        assert_eq!(
            old.superseded_by_id.as_deref(),
            Some("memory-supersession-new")
        );
        assert_eq!(new.freshness_state, FreshnessState::Current.as_str());
        assert_eq!(
            new.supersedes_id.as_deref(),
            Some("memory-supersession-old")
        );
        assert_eq!(old.fact_key, new.fact_key);
    }

    #[test]
    fn s28_correct_agent_memory_creates_approved_superseding_memory() {
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(
            repo_root.join("src/memory_correction.rs"),
            "pub fn subject() {}\n",
        )
        .expect("write source");
        let project_id = "project-memory-correction";
        create_project_database(&repo_root, project_id);
        seed_agent_run(&repo_root, project_id, "run-memory-correction");
        let source_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-memory-correction".into(),
                change_group_id: None,
                path: "src/memory_correction.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:01:00Z".into(),
            },
        )
        .expect("append source change");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-correction-original".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "The memory correction subject lives in the temporary cache.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(80),
                source_run_id: Some("run-memory-correction".into()),
                source_item_ids: vec![format!("file_change:{}", source_change.id)],
                diagnostic: None,
                created_at: "2026-05-03T00:02:00Z".into(),
            },
        )
        .expect("insert original memory");

        let correction = correct_agent_memory(
            &repo_root,
            project_id,
            "memory-correction-original",
            "The memory correction subject lives in app-data storage.",
            "2026-05-09T00:00:00Z",
        )
        .expect("correct memory");

        assert_eq!(
            correction.original.freshness_state,
            FreshnessState::Superseded.as_str()
        );
        assert_eq!(
            correction.original.superseded_by_id.as_deref(),
            Some(correction.corrected.memory_id.as_str())
        );
        assert_eq!(
            correction.corrected.supersedes_id.as_deref(),
            Some("memory-correction-original")
        );
        assert_eq!(
            correction.corrected.review_state,
            AgentMemoryReviewState::Approved
        );
        assert!(correction.corrected.enabled);
        assert!(correction
            .corrected
            .source_item_ids
            .iter()
            .any(|item| { item == "corrected-memory:memory-correction-original" }));
        assert_eq!(
            correction
                .corrected
                .diagnostic
                .as_ref()
                .map(|diagnostic| diagnostic.code.as_str()),
            Some("memory_corrected_by_user")
        );
    }

    #[test]
    fn candidate_agent_memory_supersedes_only_after_approval() {
        agent_memory_lance::reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let repo_root = tempdir.path().join("repo");
        fs::create_dir_all(repo_root.join("src")).expect("repo src dir");
        fs::write(
            repo_root.join("src/candidate_subject.rs"),
            "pub fn subject() {}\n",
        )
        .expect("write source");
        let project_id = "project-memory-candidate-supersession";
        create_project_database(&repo_root, project_id);
        seed_agent_run(&repo_root, project_id, "run-memory-candidate-old");
        let old_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-memory-candidate-old".into(),
                change_group_id: None,
                path: "src/candidate_subject.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:01:00Z".into(),
            },
        )
        .expect("append old file change");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-candidate-old".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "The freshness candidate subject lives in the old memory.".into(),
                review_state: AgentMemoryReviewState::Approved,
                enabled: true,
                confidence: Some(90),
                source_run_id: Some("run-memory-candidate-old".into()),
                source_item_ids: vec![format!("file_change:{}", old_change.id)],
                diagnostic: None,
                created_at: "2026-05-03T00:02:00Z".into(),
            },
        )
        .expect("insert old memory");

        seed_agent_run(&repo_root, project_id, "run-memory-candidate-new");
        let new_change = append_agent_file_change(
            &repo_root,
            &NewAgentFileChangeRecord {
                project_id: project_id.into(),
                run_id: "run-memory-candidate-new".into(),
                change_group_id: None,
                path: "src/candidate_subject.rs".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-03T00:03:00Z".into(),
            },
        )
        .expect("append new file change");
        insert_agent_memory(
            &repo_root,
            &NewAgentMemoryRecord {
                memory_id: "memory-candidate-new".into(),
                project_id: project_id.into(),
                agent_session_id: None,
                scope: AgentMemoryScope::Project,
                kind: AgentMemoryKind::ProjectFact,
                text: "The freshness candidate subject now lives in the new memory.".into(),
                review_state: AgentMemoryReviewState::Candidate,
                enabled: false,
                confidence: Some(95),
                source_run_id: Some("run-memory-candidate-new".into()),
                source_item_ids: vec![format!("file_change:{}", new_change.id)],
                diagnostic: None,
                created_at: "2026-05-03T00:04:00Z".into(),
            },
        )
        .expect("insert candidate memory");
        let before = list_agent_memories(&repo_root, project_id, AgentMemoryListFilter::default())
            .expect("list memories before approval");
        let old_before = before
            .iter()
            .find(|memory| memory.memory_id == "memory-candidate-old")
            .expect("old memory before approval");
        assert_eq!(old_before.freshness_state, FreshnessState::Current.as_str());
        assert!(old_before.superseded_by_id.is_none());

        update_agent_memory(
            &repo_root,
            &AgentMemoryUpdateRecord {
                project_id: project_id.into(),
                memory_id: "memory-candidate-new".into(),
                review_state: Some(AgentMemoryReviewState::Approved),
                enabled: Some(true),
                diagnostic: None,
            },
        )
        .expect("approve candidate memory");
        let after = list_agent_memories(&repo_root, project_id, AgentMemoryListFilter::default())
            .expect("list memories after approval");
        let old_after = after
            .iter()
            .find(|memory| memory.memory_id == "memory-candidate-old")
            .expect("old memory after approval");
        let new_after = after
            .iter()
            .find(|memory| memory.memory_id == "memory-candidate-new")
            .expect("new memory after approval");

        assert_eq!(
            old_after.freshness_state,
            FreshnessState::Superseded.as_str()
        );
        assert_eq!(
            old_after.superseded_by_id.as_deref(),
            Some("memory-candidate-new")
        );
        assert_eq!(
            new_after.supersedes_id.as_deref(),
            Some("memory-candidate-old")
        );
    }
}
