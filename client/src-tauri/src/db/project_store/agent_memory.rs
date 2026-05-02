use std::path::Path;

use rand::RngCore;
use sha2::{Digest, Sha256};

use crate::{auth::now_timestamp, commands::CommandError, db::database_path_for_repo};

use super::agent_core::AgentRunDiagnosticRecord;
use super::agent_embeddings::embedding_for_storage;
use super::agent_memory_lance::{
    self, AgentMemoryListFilterOwned, AgentMemoryRow, AgentMemoryUpdate, ProjectMemoryStore,
};
use super::{open_runtime_database, read_project_row};

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
        created_at: record.created_at.clone(),
        updated_at: record.created_at.clone(),
        embedding: Some(embedding.vector),
        embedding_model: Some(embedding.model),
        embedding_dimension: Some(embedding.dimension),
        embedding_version: Some(embedding.version),
    };
    store.insert(row)
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
    store.update(
        AgentMemoryUpdate {
            project_id: update.project_id.clone(),
            memory_id: update.memory_id.clone(),
            review_state: update.review_state.clone(),
            enabled: update.enabled,
            diagnostic: update.diagnostic.clone(),
        },
        now_timestamp(),
    )
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
}
