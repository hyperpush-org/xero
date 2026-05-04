use std::{collections::BTreeSet, path::Path};

use rand::RngCore;
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
use super::{load_agent_file_changes, open_runtime_database, read_project_row};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMemoryScope {
    Project,
    Session,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentMemoryKind {
    ProjectFact,
    UserPreference,
    Decision,
    SessionSummary,
    Troubleshooting,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
    let inserted = store.insert(row)?;
    if inserted.memory_id == record.memory_id {
        apply_agent_memory_supersession(&store, &inserted, &fact_key, &record.created_at)?;
        return store
            .get_by_memory_id(&inserted.memory_id)?
            .ok_or_else(|| missing_agent_memory_error(&record.project_id, &inserted.memory_id));
    }
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
    store.list_approved(agent_session_id)
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
        append_agent_file_change, create_agent_session, insert_agent_run, AgentSessionCreateRecord,
        AgentSessionRecord, NewAgentFileChangeRecord, NewAgentRunRecord,
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
