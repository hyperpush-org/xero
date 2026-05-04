use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use rusqlite::{params, OptionalExtension, Row};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, RuntimeAgentIdDto},
    db::database_path_for_repo,
};

use super::{
    agent_continuity::{
        insert_agent_retrieval_query_log, insert_agent_retrieval_result_log,
        AgentContextRedactionState, AgentRetrievalQueryLogRecord, AgentRetrievalQueryStatus,
        AgentRetrievalResultLogRecord, AgentRetrievalResultSourceKind, AgentRetrievalSearchScope,
        NewAgentRetrievalQueryLogRecord, NewAgentRetrievalResultLogRecord,
    },
    agent_embeddings::{
        cosine_similarity, default_embedding_service, embedding_with_service,
        validate_embedding_dimension, AgentEmbedding, AgentEmbeddingService,
        DEFAULT_AGENT_EMBEDDING_MODEL, DEFAULT_AGENT_EMBEDDING_VERSION,
    },
    agent_memory::refresh_agent_memory_rows,
    agent_memory::{AgentMemoryKind, AgentMemoryReviewState, AgentMemoryScope},
    agent_memory_lance::{self, AgentMemoryListFilterOwned, AgentMemoryRow, ProjectMemoryStore},
    freshness::{
        evaluate_freshness, freshness_metadata_json, freshness_update_changed,
        parse_freshness_state, source_fingerprint_paths, FreshnessMetadata,
        FreshnessRefreshSummary, FreshnessState,
    },
    open_runtime_database,
    project_record::{
        project_record_kind_sql_value, refresh_project_record_rows, ProjectRecordImportance,
        ProjectRecordKind, ProjectRecordRedactionState,
    },
    project_record_lance::{self, ProjectRecordRow, ProjectRecordStore},
    read_project_row, validate_non_empty_text,
};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentContextRetrievalFilters {
    pub record_kinds: Vec<ProjectRecordKind>,
    pub memory_kinds: Vec<AgentMemoryKind>,
    pub tags: Vec<String>,
    pub related_paths: Vec<String>,
    pub runtime_agent_id: Option<RuntimeAgentIdDto>,
    pub agent_session_id: Option<String>,
    pub created_after: Option<String>,
    pub min_importance: Option<ProjectRecordImportance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentContextRetrievalRequest {
    pub query_id: String,
    pub project_id: String,
    pub agent_session_id: Option<String>,
    pub run_id: Option<String>,
    pub runtime_agent_id: RuntimeAgentIdDto,
    pub agent_definition_id: String,
    pub agent_definition_version: u32,
    pub query_text: String,
    pub search_scope: AgentRetrievalSearchScope,
    pub filters: AgentContextRetrievalFilters,
    pub limit_count: u32,
    pub allow_keyword_fallback: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentContextRetrievalResult {
    pub result_id: String,
    pub source_kind: AgentRetrievalResultSourceKind,
    pub source_id: String,
    pub rank: u32,
    pub score: Option<f64>,
    pub snippet: String,
    pub redaction_state: AgentContextRedactionState,
    pub metadata: JsonValue,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentContextRetrievalResponse {
    pub query: AgentRetrievalQueryLogRecord,
    pub results: Vec<AgentContextRetrievalResult>,
    pub result_logs: Vec<AgentRetrievalResultLogRecord>,
    pub method: String,
    pub diagnostic: Option<JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEmbeddingBackfillSourceKind {
    ProjectRecord,
    ApprovedMemory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentEmbeddingBackfillStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentEmbeddingBackfillJobRecord {
    pub id: i64,
    pub job_id: String,
    pub project_id: String,
    pub source_kind: AgentEmbeddingBackfillSourceKind,
    pub source_id: String,
    pub source_hash: String,
    pub embedding_model: String,
    pub embedding_dimension: i32,
    pub embedding_version: String,
    pub status: AgentEmbeddingBackfillStatus,
    pub attempts: u32,
    pub diagnostic: Option<JsonValue>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewAgentEmbeddingBackfillJobRecord {
    pub job_id: String,
    pub project_id: String,
    pub source_kind: AgentEmbeddingBackfillSourceKind,
    pub source_id: String,
    pub source_hash: String,
    pub embedding_model: String,
    pub embedding_dimension: i32,
    pub embedding_version: String,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AgentEmbeddingBackfillRunRecord {
    pub queued_count: usize,
    pub succeeded_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
}

pub fn search_agent_context(
    repo_root: &Path,
    request: AgentContextRetrievalRequest,
) -> Result<AgentContextRetrievalResponse, CommandError> {
    search_agent_context_with_embedding_service(
        repo_root,
        request,
        Some(default_embedding_service()),
    )
}

pub fn search_agent_context_with_embedding_service(
    repo_root: &Path,
    request: AgentContextRetrievalRequest,
    embedding_service: Option<&dyn AgentEmbeddingService>,
) -> Result<AgentContextRetrievalResponse, CommandError> {
    validate_retrieval_request(&request)?;
    if let Some(service) = embedding_service {
        if let Err(error) = validate_embedding_dimension(service.dimension()) {
            let _ = log_failed_retrieval_query(repo_root, &request, &error);
            return Err(error);
        }
    } else if !request.allow_keyword_fallback {
        let error = CommandError::retryable(
            "agent_retrieval_embedding_unavailable",
            "Xero could not run semantic retrieval because no embedding service is configured and keyword fallback is disabled.",
        );
        let _ = log_failed_retrieval_query(repo_root, &request, &error);
        return Err(error);
    }

    let (record_store, memory_store) = open_retrieval_stores(repo_root, &request.project_id)?;
    let query_embedding = embedding_service
        .map(|service| embedding_with_service(service, &request.query_text))
        .transpose()?;
    let query_tokens = token_set(&request.query_text);
    let freshness_diagnostics =
        refresh_retrieval_freshness(repo_root, &record_store, &memory_store, &request)?;
    let collection = collect_candidates(
        &record_store,
        &memory_store,
        &request,
        query_embedding.as_ref(),
        &query_tokens,
    )?;
    let candidates = collection.candidates;

    let fallback_used = query_embedding.is_none()
        || candidates
            .iter()
            .any(|candidate| candidate.embedding.as_ref().is_none());
    let method = if fallback_used {
        "keyword_fallback"
    } else {
        "hybrid"
    }
    .to_string();
    let diagnostic = retrieval_diagnostic(
        fallback_used.then(|| {
            json!({
            "code": "agent_retrieval_keyword_fallback",
            "message": "Xero used deterministic keyword, metadata, recency, and importance scoring for records without usable embeddings."
        })
        }),
        freshness_diagnostics,
        collection.blocked_excluded_count,
        collection.freshness_reason_counts.clone(),
    );

    let filters_json = retrieval_filters_json(&request.filters);
    let completed_at = now_timestamp();
    let query = insert_agent_retrieval_query_log(
        repo_root,
        &NewAgentRetrievalQueryLogRecord {
            query_id: request.query_id.clone(),
            project_id: request.project_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            run_id: request.run_id.clone(),
            runtime_agent_id: request.runtime_agent_id,
            agent_definition_id: request.agent_definition_id.clone(),
            agent_definition_version: request.agent_definition_version,
            query_text: request.query_text.clone(),
            search_scope: request.search_scope.clone(),
            filters: filters_json,
            limit_count: request.limit_count,
            status: AgentRetrievalQueryStatus::Succeeded,
            diagnostic: diagnostic.clone(),
            created_at: request.created_at.clone(),
            completed_at: Some(completed_at.clone()),
        },
    )?;

    let mut results = Vec::new();
    let mut result_logs = Vec::new();
    for (index, candidate) in candidates.into_iter().enumerate() {
        let rank = (index as u32) + 1;
        let result_id = format!("{}-result-{rank}", request.query_id);
        let result = AgentContextRetrievalResult {
            result_id: result_id.clone(),
            source_kind: candidate.source_kind.clone(),
            source_id: candidate.source_id.clone(),
            rank,
            score: Some(candidate.score),
            snippet: candidate.snippet.clone(),
            redaction_state: candidate.redaction_state.clone(),
            metadata: candidate.metadata.clone(),
        };
        let log = insert_agent_retrieval_result_log(
            repo_root,
            &NewAgentRetrievalResultLogRecord {
                project_id: request.project_id.clone(),
                query_id: request.query_id.clone(),
                result_id,
                source_kind: candidate.source_kind,
                source_id: candidate.source_id,
                rank,
                score: Some(candidate.score),
                snippet: candidate.snippet,
                redaction_state: candidate.redaction_state,
                metadata: Some(candidate.metadata),
                created_at: completed_at.clone(),
            },
        )?;
        results.push(result);
        result_logs.push(log);
    }

    Ok(AgentContextRetrievalResponse {
        query,
        results,
        result_logs,
        method,
        diagnostic,
    })
}

pub fn enqueue_missing_agent_embedding_backfill_jobs(
    repo_root: &Path,
    project_id: &str,
    now: &str,
) -> Result<Vec<AgentEmbeddingBackfillJobRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_embedding_backfill_project_required",
    )?;
    validate_non_empty_text(now, "now", "agent_embedding_backfill_now_required")?;
    let (record_store, memory_store) = open_retrieval_stores(repo_root, project_id)?;
    let mut jobs = Vec::new();

    for row in record_store.list_rows()? {
        if row.redaction_state
            == project_record_redaction_state_value(&ProjectRecordRedactionState::Blocked)
            || backfill_should_skip_for_freshness(&row.freshness_state)
            || embedding_is_current(
                row.embedding.as_ref(),
                row.embedding_model.as_deref(),
                row.embedding_dimension,
                row.embedding_version.as_deref(),
            )
        {
            continue;
        }
        jobs.push(enqueue_agent_embedding_backfill_job(
            repo_root,
            &NewAgentEmbeddingBackfillJobRecord {
                job_id: embedding_backfill_job_id(
                    AgentEmbeddingBackfillSourceKind::ProjectRecord,
                    &row.record_id,
                    &row.text_hash,
                ),
                project_id: project_id.to_string(),
                source_kind: AgentEmbeddingBackfillSourceKind::ProjectRecord,
                source_id: row.record_id,
                source_hash: row.text_hash,
                embedding_model: DEFAULT_AGENT_EMBEDDING_MODEL.to_string(),
                embedding_dimension: super::agent_embeddings::AGENT_RETRIEVAL_EMBEDDING_DIM,
                embedding_version: DEFAULT_AGENT_EMBEDDING_VERSION.to_string(),
                created_at: now.to_string(),
            },
        )?);
    }

    for row in memory_store.list_all_rows()? {
        if row.review_state != AgentMemoryReviewState::Approved
            || !row.enabled
            || backfill_should_skip_for_freshness(&row.freshness_state)
            || embedding_is_current(
                row.embedding.as_ref(),
                row.embedding_model.as_deref(),
                row.embedding_dimension,
                row.embedding_version.as_deref(),
            )
        {
            continue;
        }
        jobs.push(enqueue_agent_embedding_backfill_job(
            repo_root,
            &NewAgentEmbeddingBackfillJobRecord {
                job_id: embedding_backfill_job_id(
                    AgentEmbeddingBackfillSourceKind::ApprovedMemory,
                    &row.memory_id,
                    &row.text_hash,
                ),
                project_id: project_id.to_string(),
                source_kind: AgentEmbeddingBackfillSourceKind::ApprovedMemory,
                source_id: row.memory_id,
                source_hash: row.text_hash,
                embedding_model: DEFAULT_AGENT_EMBEDDING_MODEL.to_string(),
                embedding_dimension: super::agent_embeddings::AGENT_RETRIEVAL_EMBEDDING_DIM,
                embedding_version: DEFAULT_AGENT_EMBEDDING_VERSION.to_string(),
                created_at: now.to_string(),
            },
        )?);
    }

    Ok(jobs)
}

pub fn enqueue_agent_embedding_backfill_job(
    repo_root: &Path,
    record: &NewAgentEmbeddingBackfillJobRecord,
) -> Result<AgentEmbeddingBackfillJobRecord, CommandError> {
    validate_backfill_job(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    connection
        .execute(
            r#"
            INSERT INTO agent_embedding_backfill_jobs (
                job_id,
                project_id,
                source_kind,
                source_id,
                source_hash,
                embedding_model,
                embedding_dimension,
                embedding_version,
                status,
                attempts,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'pending', 0, ?9, ?9)
            ON CONFLICT(project_id, source_kind, source_id, embedding_model, embedding_version)
            DO NOTHING
            "#,
            params![
                record.job_id,
                record.project_id,
                backfill_source_kind_sql_value(&record.source_kind),
                record.source_id,
                record.source_hash,
                record.embedding_model,
                record.embedding_dimension,
                record.embedding_version,
                record.created_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_embedding_backfill_job_insert_failed",
                format!(
                    "Xero could not enqueue an embedding backfill job in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    get_agent_embedding_backfill_job(
        repo_root,
        &record.project_id,
        &record.source_kind,
        &record.source_id,
        &record.embedding_model,
        &record.embedding_version,
    )?
    .ok_or_else(|| {
        CommandError::system_fault(
            "agent_embedding_backfill_job_insert_missing",
            "Xero queued an embedding backfill job but could not load it back.",
        )
    })
}

pub fn run_agent_embedding_backfill_jobs(
    repo_root: &Path,
    project_id: &str,
    limit: u32,
    now: &str,
) -> Result<AgentEmbeddingBackfillRunRecord, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_embedding_backfill_project_required",
    )?;
    validate_non_empty_text(now, "now", "agent_embedding_backfill_now_required")?;
    let limit = limit.max(1);
    let jobs = list_pending_agent_embedding_backfill_jobs(repo_root, project_id, limit)?;
    let (record_store, memory_store) = open_retrieval_stores(repo_root, project_id)?;
    let mut run = AgentEmbeddingBackfillRunRecord::default();

    for job in jobs {
        run.queued_count += 1;
        update_backfill_job_status(
            repo_root,
            &job.project_id,
            &job.job_id,
            AgentEmbeddingBackfillStatus::Running,
            job.attempts.saturating_add(1),
            None,
            now,
            None,
        )?;
        let result = apply_backfill_job(repo_root, &record_store, &memory_store, &job, now);
        match result {
            Ok(BackfillOutcome::Succeeded) => {
                run.succeeded_count += 1;
                update_backfill_job_status(
                    repo_root,
                    &job.project_id,
                    &job.job_id,
                    AgentEmbeddingBackfillStatus::Succeeded,
                    job.attempts.saturating_add(1),
                    None,
                    now,
                    Some(now),
                )?;
            }
            Ok(BackfillOutcome::Skipped(diagnostic)) => {
                run.skipped_count += 1;
                update_backfill_job_status(
                    repo_root,
                    &job.project_id,
                    &job.job_id,
                    AgentEmbeddingBackfillStatus::Skipped,
                    job.attempts.saturating_add(1),
                    Some(diagnostic),
                    now,
                    Some(now),
                )?;
            }
            Err(error) => {
                run.failed_count += 1;
                update_backfill_job_status(
                    repo_root,
                    &job.project_id,
                    &job.job_id,
                    AgentEmbeddingBackfillStatus::Failed,
                    job.attempts.saturating_add(1),
                    Some(json!({"code": error.code, "message": error.message})),
                    now,
                    Some(now),
                )?;
            }
        }
    }

    Ok(run)
}

pub fn list_agent_embedding_backfill_jobs(
    repo_root: &Path,
    project_id: &str,
) -> Result<Vec<AgentEmbeddingBackfillJobRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "agent_embedding_backfill_project_required",
    )?;
    let connection = open_backfill_database(repo_root)?;
    let mut statement = connection
        .prepare(
            backfill_job_select_sql("WHERE project_id = ?1 ORDER BY created_at DESC, id DESC")
                .as_str(),
        )
        .map_err(map_backfill_read_error)?;
    let rows = statement
        .query_map(params![project_id], read_backfill_job_row)
        .map_err(map_backfill_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_backfill_read_error)?
        .into_iter()
        .collect()
}

fn refresh_retrieval_freshness(
    repo_root: &Path,
    record_store: &ProjectRecordStore,
    memory_store: &ProjectMemoryStore,
    request: &AgentContextRetrievalRequest,
) -> Result<FreshnessRefreshSummary, CommandError> {
    let mut summary = FreshnessRefreshSummary::default();
    if matches!(
        request.search_scope,
        AgentRetrievalSearchScope::ProjectRecords
            | AgentRetrievalSearchScope::HybridContext
            | AgentRetrievalSearchScope::Handoffs
    ) {
        summary.merge(refresh_project_record_rows(
            repo_root,
            record_store,
            None,
            &request.created_at,
        )?);
    }
    if matches!(
        request.search_scope,
        AgentRetrievalSearchScope::ApprovedMemory | AgentRetrievalSearchScope::HybridContext
    ) {
        summary.merge(refresh_agent_memory_rows(
            repo_root,
            memory_store,
            None,
            &request.created_at,
        )?);
    }
    Ok(summary)
}

fn collect_candidates(
    record_store: &ProjectRecordStore,
    memory_store: &ProjectMemoryStore,
    request: &AgentContextRetrievalRequest,
    query_embedding: Option<&AgentEmbedding>,
    query_tokens: &BTreeSet<String>,
) -> Result<CandidateCollection, CommandError> {
    let mut candidates = Vec::new();
    let mut blocked_excluded_count = 0_usize;
    let mut freshness_reason_counts = BTreeMap::new();
    if matches!(
        request.search_scope,
        AgentRetrievalSearchScope::ProjectRecords
            | AgentRetrievalSearchScope::HybridContext
            | AgentRetrievalSearchScope::Handoffs
    ) {
        for row in record_store.list_rows()? {
            if row.redaction_state
                == project_record_redaction_state_value(&ProjectRecordRedactionState::Blocked)
            {
                blocked_excluded_count += 1;
            }
            if let Some(candidate) =
                project_record_candidate(row, request, query_embedding, query_tokens)?
            {
                record_candidate_freshness_reason(&candidate, &mut freshness_reason_counts);
                candidates.push(candidate);
            }
        }
    }
    if matches!(
        request.search_scope,
        AgentRetrievalSearchScope::ApprovedMemory | AgentRetrievalSearchScope::HybridContext
    ) {
        let session_filter = request
            .filters
            .agent_session_id
            .as_deref()
            .or(request.agent_session_id.as_deref());
        for row in memory_store.list_rows(session_filter, AgentMemoryListFilterOwned::default())? {
            if let Some(candidate) = memory_candidate(row, request, query_embedding, query_tokens)?
            {
                record_candidate_freshness_reason(&candidate, &mut freshness_reason_counts);
                candidates.push(candidate);
            }
        }
    }

    candidates.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.source_id.cmp(&right.source_id))
    });
    candidates.truncate(request.limit_count as usize);
    Ok(CandidateCollection {
        candidates,
        blocked_excluded_count,
        freshness_reason_counts,
    })
}

fn project_record_candidate(
    row: ProjectRecordRow,
    request: &AgentContextRetrievalRequest,
    query_embedding: Option<&AgentEmbedding>,
    query_tokens: &BTreeSet<String>,
) -> Result<Option<SearchCandidate>, CommandError> {
    if row.redaction_state
        == project_record_redaction_state_value(&ProjectRecordRedactionState::Blocked)
        || row.visibility == "memory_candidate"
        || (request.search_scope == AgentRetrievalSearchScope::Handoffs
            && row.record_kind != project_record_kind_sql_value(&ProjectRecordKind::AgentHandoff))
        || (!request.filters.record_kinds.is_empty()
            && !request
                .filters
                .record_kinds
                .iter()
                .any(|kind| project_record_kind_sql_value(kind) == row.record_kind))
        || request
            .filters
            .runtime_agent_id
            .is_some_and(|runtime_agent_id| row.runtime_agent_id != runtime_agent_id)
        || request
            .filters
            .agent_session_id
            .as_deref()
            .is_some_and(|session_id| row.agent_session_id.as_deref() != Some(session_id))
        || request
            .filters
            .created_after
            .as_deref()
            .is_some_and(|created_after| row.created_at.as_str() < created_after)
        || request
            .filters
            .min_importance
            .as_ref()
            .is_some_and(|importance| {
                importance_rank(&row.importance) < project_importance_rank(importance)
            })
    {
        return Ok(None);
    }

    let tags = parse_string_array(&row.tags_json, "tags")?;
    if !request.filters.tags.is_empty()
        && !request
            .filters
            .tags
            .iter()
            .all(|filter| tags.iter().any(|tag| tag == filter))
    {
        return Ok(None);
    }
    let related_paths = parse_string_array(&row.related_paths_json, "relatedPaths")?;
    let source_item_ids = parse_string_array(&row.source_item_ids_json, "sourceItemIds")?;
    if !request.filters.related_paths.is_empty()
        && !request.filters.related_paths.iter().any(|filter| {
            related_paths
                .iter()
                .any(|path| path == filter || path.ends_with(filter))
        })
    {
        return Ok(None);
    }

    let body = format!(
        "{}\n{}\n{}\n{}\n{}",
        row.title,
        row.summary,
        row.text,
        tags.join(" "),
        related_paths.join(" ")
    );
    let keyword_score = keyword_score(query_tokens, &body);
    let vector_score = semantic_score(
        query_embedding,
        row.embedding.as_ref(),
        row.embedding_dimension,
        row.embedding_model.as_deref(),
        row.embedding_version.as_deref(),
    );
    if keyword_score == 0.0 && vector_score == 0.0 {
        return Ok(None);
    }
    let score = keyword_score.mul_add(2.0, vector_score)
        + f64::from(importance_rank(&row.importance)) * 0.05;
    let score = (score + freshness_score_adjustment(&row.freshness_state)).max(0.0);
    let (snippet, snippet_redaction) = retrieval_snippet(&row.text);
    let redaction_state = if row.redaction_state
        == project_record_redaction_state_value(&ProjectRecordRedactionState::Redacted)
    {
        AgentContextRedactionState::Redacted
    } else {
        snippet_redaction
    };
    let source_kind =
        if row.record_kind == project_record_kind_sql_value(&ProjectRecordKind::AgentHandoff) {
            AgentRetrievalResultSourceKind::Handoff
        } else {
            AgentRetrievalResultSourceKind::ProjectRecord
        };
    let freshness = freshness_metadata_json(FreshnessMetadata {
        freshness_state: &row.freshness_state,
        freshness_checked_at: row.freshness_checked_at.as_deref(),
        stale_reason: row.stale_reason.as_deref(),
        source_fingerprints_json: &row.source_fingerprints_json,
        supersedes_id: row.supersedes_id.as_deref(),
        superseded_by_id: row.superseded_by_id.as_deref(),
        invalidated_at: row.invalidated_at.as_deref(),
        fact_key: row.fact_key.as_deref(),
    })?;
    let trust = json!({
        "freshnessState": row.freshness_state,
        "staleReason": row.stale_reason,
        "checkedAt": row.freshness_checked_at,
        "sourceFingerprints": freshness.get("sourceFingerprints").cloned().unwrap_or(JsonValue::Array(Vec::new())),
        "supersedesId": row.supersedes_id,
        "supersededById": row.superseded_by_id,
        "invalidatedAt": row.invalidated_at,
        "factKey": row.fact_key,
        "confidence": row.confidence,
        "sourceRunId": row.run_id,
        "sourceItemIds": source_item_ids,
        "relatedPaths": related_paths,
    });
    Ok(Some(SearchCandidate {
        source_kind,
        source_id: row.record_id,
        score,
        snippet,
        redaction_state,
        created_at: row.created_at,
        embedding: row.embedding,
        metadata: json!({
            "title": row.title,
            "recordKind": row.record_kind,
            "runtimeAgentId": row.runtime_agent_id.as_str(),
            "agentSessionId": row.agent_session_id,
            "runId": trust["sourceRunId"].clone(),
            "tags": tags,
            "relatedPaths": trust["relatedPaths"].clone(),
            "importance": row.importance,
            "confidence": trust["confidence"].clone(),
            "sourceItemIds": trust["sourceItemIds"].clone(),
            "embeddingPresent": row.embedding_model.is_some(),
            "embeddingModel": row.embedding_model,
            "embeddingDimension": row.embedding_dimension,
            "embeddingVersion": row.embedding_version,
            "keywordScore": keyword_score,
            "semanticScore": vector_score,
            "freshness": freshness,
            "trust": trust
        }),
    }))
}

fn memory_candidate(
    row: AgentMemoryRow,
    request: &AgentContextRetrievalRequest,
    query_embedding: Option<&AgentEmbedding>,
    query_tokens: &BTreeSet<String>,
) -> Result<Option<SearchCandidate>, CommandError> {
    if row.review_state != AgentMemoryReviewState::Approved
        || !row.enabled
        || !request.filters.tags.is_empty()
        || !request.filters.related_paths.is_empty()
        || request.filters.runtime_agent_id.is_some()
        || request.filters.min_importance.is_some()
        || (!request.filters.memory_kinds.is_empty()
            && !request
                .filters
                .memory_kinds
                .iter()
                .any(|kind| kind == &row.kind))
        || request
            .filters
            .created_after
            .as_deref()
            .is_some_and(|created_after| row.created_at.as_str() < created_after)
    {
        return Ok(None);
    }
    let body = format!("{:?}\n{:?}\n{}", row.scope, row.kind, row.text);
    let keyword_score = keyword_score(query_tokens, &body);
    let vector_score = semantic_score(
        query_embedding,
        row.embedding.as_ref(),
        row.embedding_dimension,
        row.embedding_model.as_deref(),
        row.embedding_version.as_deref(),
    );
    if keyword_score == 0.0 && vector_score == 0.0 {
        return Ok(None);
    }
    let score = keyword_score.mul_add(2.0, vector_score)
        + row
            .confidence
            .map(|value| f64::from(value) / 500.0)
            .unwrap_or(0.0);
    let score = (score + freshness_score_adjustment(&row.freshness_state)).max(0.0);
    let (snippet, redaction_state) = retrieval_snippet(&row.text);
    let scope = memory_scope_sql_value(&row.scope);
    let kind = memory_kind_sql_value(&row.kind);
    let freshness = freshness_metadata_json(FreshnessMetadata {
        freshness_state: &row.freshness_state,
        freshness_checked_at: row.freshness_checked_at.as_deref(),
        stale_reason: row.stale_reason.as_deref(),
        source_fingerprints_json: &row.source_fingerprints_json,
        supersedes_id: row.supersedes_id.as_deref(),
        superseded_by_id: row.superseded_by_id.as_deref(),
        invalidated_at: row.invalidated_at.as_deref(),
        fact_key: row.fact_key.as_deref(),
    })?;
    let related_paths = source_fingerprint_paths(&row.source_fingerprints_json)?;
    let trust = json!({
        "freshnessState": row.freshness_state,
        "staleReason": row.stale_reason,
        "checkedAt": row.freshness_checked_at,
        "sourceFingerprints": freshness.get("sourceFingerprints").cloned().unwrap_or(JsonValue::Array(Vec::new())),
        "supersedesId": row.supersedes_id,
        "supersededById": row.superseded_by_id,
        "invalidatedAt": row.invalidated_at,
        "factKey": row.fact_key,
        "confidence": row.confidence,
        "sourceRunId": row.source_run_id,
        "sourceItemIds": row.source_item_ids,
        "relatedPaths": related_paths,
    });
    Ok(Some(SearchCandidate {
        source_kind: AgentRetrievalResultSourceKind::ApprovedMemory,
        source_id: row.memory_id,
        score,
        snippet,
        redaction_state,
        created_at: row.created_at,
        embedding: row.embedding,
        metadata: json!({
            "scope": scope,
            "memoryKind": kind,
            "agentSessionId": row.agent_session_id,
            "sourceRunId": trust["sourceRunId"].clone(),
            "sourceItemIds": trust["sourceItemIds"].clone(),
            "relatedPaths": trust["relatedPaths"].clone(),
            "confidence": trust["confidence"].clone(),
            "embeddingPresent": row.embedding_model.is_some(),
            "embeddingModel": row.embedding_model,
            "embeddingDimension": row.embedding_dimension,
            "embeddingVersion": row.embedding_version,
            "keywordScore": keyword_score,
            "semanticScore": vector_score,
            "freshness": freshness,
            "trust": trust
        }),
    }))
}

#[derive(Debug)]
struct CandidateCollection {
    candidates: Vec<SearchCandidate>,
    blocked_excluded_count: usize,
    freshness_reason_counts: BTreeMap<String, usize>,
}

#[derive(Debug)]
struct SearchCandidate {
    source_kind: AgentRetrievalResultSourceKind,
    source_id: String,
    score: f64,
    snippet: String,
    redaction_state: AgentContextRedactionState,
    created_at: String,
    embedding: Option<Vec<f32>>,
    metadata: JsonValue,
}

fn record_candidate_freshness_reason(
    candidate: &SearchCandidate,
    freshness_reason_counts: &mut BTreeMap<String, usize>,
) {
    let Some(freshness) = candidate
        .metadata
        .get("freshness")
        .and_then(JsonValue::as_object)
    else {
        return;
    };
    let state = freshness
        .get("state")
        .and_then(JsonValue::as_str)
        .unwrap_or("source_unknown");
    let reason = freshness
        .get("staleReason")
        .and_then(JsonValue::as_str)
        .filter(|reason| !reason.trim().is_empty())
        .unwrap_or("No stale reason recorded.");
    if matches!(
        state,
        "stale" | "source_missing" | "superseded" | "source_unknown"
    ) {
        let key = format!("{state}: {reason}");
        *freshness_reason_counts.entry(key).or_insert(0) += 1;
    }
}

enum BackfillOutcome {
    Succeeded,
    Skipped(JsonValue),
}

fn apply_backfill_job(
    repo_root: &Path,
    record_store: &ProjectRecordStore,
    memory_store: &ProjectMemoryStore,
    job: &AgentEmbeddingBackfillJobRecord,
    now: &str,
) -> Result<BackfillOutcome, CommandError> {
    match job.source_kind {
        AgentEmbeddingBackfillSourceKind::ProjectRecord => {
            let Some(row) = record_store
                .list_rows()?
                .into_iter()
                .find(|row| row.record_id == job.source_id)
            else {
                return Ok(BackfillOutcome::Skipped(json!({
                    "code": "agent_embedding_backfill_source_missing",
                    "message": "The project record no longer exists.",
                    "freshnessState": JsonValue::Null
                })));
            };
            let row = refresh_project_record_backfill_freshness(repo_root, record_store, row, now)?;
            if row.redaction_state
                == project_record_redaction_state_value(&ProjectRecordRedactionState::Blocked)
            {
                return Ok(BackfillOutcome::Skipped(
                    backfill_project_record_diagnostic(
                        &row,
                        "agent_embedding_backfill_source_blocked",
                        "The project record is redaction-blocked.",
                    ),
                ));
            }
            if row.text_hash != job.source_hash {
                return Ok(BackfillOutcome::Skipped(
                    backfill_project_record_diagnostic(
                        &row,
                        "agent_embedding_backfill_source_hash_mismatch",
                        "The project record text changed after this embedding backfill job was queued.",
                    ),
                ));
            }
            if backfill_should_skip_for_freshness(&row.freshness_state) {
                return Ok(BackfillOutcome::Skipped(
                    backfill_project_record_diagnostic(
                        &row,
                        "agent_embedding_backfill_source_not_fresh",
                        "The project record is not factually current, so Xero skipped embedding backfill.",
                    ),
                ));
            }
            let embedding = embedding_with_service(
                default_embedding_service(),
                &format!("{}\n{}\n{}", row.title, row.summary, row.text),
            )?;
            record_store.update_embedding(
                &row.record_id,
                embedding.vector,
                embedding.model,
                embedding.dimension,
                embedding.version,
                now.to_string(),
            )?;
        }
        AgentEmbeddingBackfillSourceKind::ApprovedMemory => {
            let Some(row) = memory_store
                .list_all_rows()?
                .into_iter()
                .find(|row| row.memory_id == job.source_id)
            else {
                return Ok(BackfillOutcome::Skipped(json!({
                    "code": "agent_embedding_backfill_source_missing",
                    "message": "The approved memory no longer exists.",
                    "freshnessState": JsonValue::Null
                })));
            };
            let row = refresh_memory_backfill_freshness(repo_root, memory_store, row, now)?;
            if row.review_state != AgentMemoryReviewState::Approved || !row.enabled {
                return Ok(BackfillOutcome::Skipped(backfill_memory_diagnostic(
                    &row,
                    "agent_embedding_backfill_source_not_approved",
                    "The memory is no longer approved and enabled.",
                )));
            }
            if row.text_hash != job.source_hash {
                return Ok(BackfillOutcome::Skipped(backfill_memory_diagnostic(
                    &row,
                    "agent_embedding_backfill_source_hash_mismatch",
                    "The approved memory text changed after this embedding backfill job was queued.",
                )));
            }
            if backfill_should_skip_for_freshness(&row.freshness_state) {
                return Ok(BackfillOutcome::Skipped(backfill_memory_diagnostic(
                    &row,
                    "agent_embedding_backfill_source_not_fresh",
                    "The approved memory is not factually current, so Xero skipped embedding backfill.",
                )));
            }
            let embedding = embedding_with_service(default_embedding_service(), &row.text)?;
            memory_store.update_embedding(
                &row.memory_id,
                embedding.vector,
                embedding.model,
                embedding.dimension,
                embedding.version,
                now.to_string(),
            )?;
        }
    }
    Ok(BackfillOutcome::Succeeded)
}

fn refresh_project_record_backfill_freshness(
    repo_root: &Path,
    record_store: &ProjectRecordStore,
    row: ProjectRecordRow,
    checked_at: &str,
) -> Result<ProjectRecordRow, CommandError> {
    let update = evaluate_freshness(
        repo_root,
        parse_freshness_state(&row.freshness_state),
        row.invalidated_at.as_deref(),
        &row.source_fingerprints_json,
        checked_at,
        row.redaction_state
            == project_record_redaction_state_value(&ProjectRecordRedactionState::Blocked),
    )?;
    if freshness_update_changed(
        &row.freshness_state,
        row.freshness_checked_at.as_deref(),
        row.stale_reason.as_deref(),
        &row.source_fingerprints_json,
        row.invalidated_at.as_deref(),
        &update,
    ) {
        return Ok(record_store
            .update_freshness(&row.record_id, update)?
            .unwrap_or(row));
    }
    Ok(row)
}

fn refresh_memory_backfill_freshness(
    repo_root: &Path,
    memory_store: &ProjectMemoryStore,
    row: AgentMemoryRow,
    checked_at: &str,
) -> Result<AgentMemoryRow, CommandError> {
    let update = evaluate_freshness(
        repo_root,
        parse_freshness_state(&row.freshness_state),
        row.invalidated_at.as_deref(),
        &row.source_fingerprints_json,
        checked_at,
        false,
    )?;
    if freshness_update_changed(
        &row.freshness_state,
        row.freshness_checked_at.as_deref(),
        row.stale_reason.as_deref(),
        &row.source_fingerprints_json,
        row.invalidated_at.as_deref(),
        &update,
    ) {
        return Ok(memory_store
            .update_freshness(&row.memory_id, update)?
            .unwrap_or(row));
    }
    Ok(row)
}

fn backfill_should_skip_for_freshness(freshness_state: &str) -> bool {
    matches!(
        parse_freshness_state(freshness_state),
        FreshnessState::Blocked
            | FreshnessState::Stale
            | FreshnessState::SourceMissing
            | FreshnessState::Superseded
    )
}

fn backfill_project_record_diagnostic(
    row: &ProjectRecordRow,
    code: &'static str,
    message: &'static str,
) -> JsonValue {
    json!({
        "code": code,
        "message": message,
        "freshnessState": row.freshness_state.as_str(),
        "staleReason": row.stale_reason.as_deref(),
        "freshnessCheckedAt": row.freshness_checked_at.as_deref(),
        "invalidatedAt": row.invalidated_at.as_deref(),
        "supersedesId": row.supersedes_id.as_deref(),
        "supersededById": row.superseded_by_id.as_deref(),
    })
}

fn backfill_memory_diagnostic(
    row: &AgentMemoryRow,
    code: &'static str,
    message: &'static str,
) -> JsonValue {
    json!({
        "code": code,
        "message": message,
        "freshnessState": row.freshness_state.as_str(),
        "staleReason": row.stale_reason.as_deref(),
        "freshnessCheckedAt": row.freshness_checked_at.as_deref(),
        "invalidatedAt": row.invalidated_at.as_deref(),
        "supersedesId": row.supersedes_id.as_deref(),
        "supersededById": row.superseded_by_id.as_deref(),
    })
}

fn log_failed_retrieval_query(
    repo_root: &Path,
    request: &AgentContextRetrievalRequest,
    error: &CommandError,
) -> Result<AgentRetrievalQueryLogRecord, CommandError> {
    let now = now_timestamp();
    insert_agent_retrieval_query_log(
        repo_root,
        &NewAgentRetrievalQueryLogRecord {
            query_id: request.query_id.clone(),
            project_id: request.project_id.clone(),
            agent_session_id: request.agent_session_id.clone(),
            run_id: request.run_id.clone(),
            runtime_agent_id: request.runtime_agent_id,
            agent_definition_id: request.agent_definition_id.clone(),
            agent_definition_version: request.agent_definition_version,
            query_text: request.query_text.clone(),
            search_scope: request.search_scope.clone(),
            filters: retrieval_filters_json(&request.filters),
            limit_count: request.limit_count.max(1),
            status: AgentRetrievalQueryStatus::Failed,
            diagnostic: Some(json!({"code": error.code, "message": error.message})),
            created_at: request.created_at.clone(),
            completed_at: Some(now),
        },
    )
}

fn open_retrieval_stores(
    repo_root: &Path,
    project_id: &str,
) -> Result<(ProjectRecordStore, ProjectMemoryStore), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    drop(connection);
    Ok((
        project_record_lance::open_for_database_path(&database_path, project_id),
        agent_memory_lance::open_for_database_path(&database_path, project_id),
    ))
}

fn validate_retrieval_request(request: &AgentContextRetrievalRequest) -> Result<(), CommandError> {
    validate_non_empty_text(
        &request.query_id,
        "queryId",
        "agent_retrieval_query_id_required",
    )?;
    validate_non_empty_text(
        &request.project_id,
        "projectId",
        "agent_retrieval_project_required",
    )?;
    validate_non_empty_text(
        &request.query_text,
        "queryText",
        "agent_retrieval_query_text_required",
    )?;
    validate_non_empty_text(
        &request.agent_definition_id,
        "agentDefinitionId",
        "agent_retrieval_definition_required",
    )?;
    if request.agent_definition_version == 0 {
        return Err(CommandError::invalid_request("agentDefinitionVersion"));
    }
    validate_non_empty_text(
        &request.created_at,
        "createdAt",
        "agent_retrieval_created_at_required",
    )?;
    if request.limit_count == 0 {
        return Err(CommandError::invalid_request("limitCount"));
    }
    Ok(())
}

fn validate_backfill_job(record: &NewAgentEmbeddingBackfillJobRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.job_id,
        "jobId",
        "agent_embedding_backfill_job_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "agent_embedding_backfill_project_required",
    )?;
    validate_non_empty_text(
        &record.source_id,
        "sourceId",
        "agent_embedding_backfill_source_required",
    )?;
    validate_non_empty_text(
        &record.source_hash,
        "sourceHash",
        "agent_embedding_backfill_hash_required",
    )?;
    validate_non_empty_text(
        &record.embedding_model,
        "embeddingModel",
        "agent_embedding_backfill_model_required",
    )?;
    validate_embedding_dimension(record.embedding_dimension)?;
    validate_non_empty_text(
        &record.embedding_version,
        "embeddingVersion",
        "agent_embedding_backfill_version_required",
    )?;
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "agent_embedding_backfill_created_at_required",
    )?;
    Ok(())
}

fn retrieval_diagnostic(
    base: Option<JsonValue>,
    mut freshness: FreshnessRefreshSummary,
    blocked_excluded_count: usize,
    freshness_reason_counts: BTreeMap<String, usize>,
) -> Option<JsonValue> {
    freshness.blocked_count = freshness.blocked_count.max(blocked_excluded_count);
    let mut freshness_json = freshness.as_json();
    if let Some(object) = freshness_json.as_object_mut() {
        object.insert(
            "reasonCounts".into(),
            json!(freshness_reason_counts
                .iter()
                .map(|(reason, count)| json!({
                    "reason": reason,
                    "count": count,
                }))
                .collect::<Vec<_>>()),
        );
    }
    match base {
        Some(mut diagnostic) => {
            if let Some(object) = diagnostic.as_object_mut() {
                object.insert("freshnessDiagnostics".into(), freshness_json);
                Some(diagnostic)
            } else {
                Some(json!({
                    "detail": diagnostic,
                    "freshnessDiagnostics": freshness_json,
                }))
            }
        }
        None => Some(json!({
            "freshnessDiagnostics": freshness_json,
        })),
    }
}

fn retrieval_filters_json(filters: &AgentContextRetrievalFilters) -> JsonValue {
    json!({
        "recordKinds": filters.record_kinds.iter().map(project_record_kind_sql_value).collect::<Vec<_>>(),
        "memoryKinds": filters.memory_kinds.iter().map(memory_kind_sql_value).collect::<Vec<_>>(),
        "tags": filters.tags.clone(),
        "relatedPaths": filters.related_paths.clone(),
        "runtimeAgentId": filters.runtime_agent_id.map(|id| id.as_str()),
        "agentSessionId": filters.agent_session_id.clone(),
        "createdAfter": filters.created_after.clone(),
        "minImportance": filters.min_importance.as_ref().map(project_record_importance_sql_value),
    })
}

fn retrieval_snippet(text: &str) -> (String, AgentContextRedactionState) {
    if super::find_prohibited_runtime_persistence_content(text).is_some() {
        return ("[redacted]".into(), AgentContextRedactionState::Redacted);
    }
    let trimmed = text.trim();
    let snippet = if trimmed.chars().count() > 320 {
        let mut shortened = trimmed.chars().take(319).collect::<String>();
        shortened.push_str("...");
        shortened
    } else {
        trimmed.to_string()
    };
    (
        if snippet.is_empty() {
            "(empty retrieval snippet)".into()
        } else {
            snippet
        },
        AgentContextRedactionState::Clean,
    )
}

fn semantic_score(
    query_embedding: Option<&AgentEmbedding>,
    row_embedding: Option<&Vec<f32>>,
    row_dimension: Option<i32>,
    row_model: Option<&str>,
    row_version: Option<&str>,
) -> f64 {
    let Some(query_embedding) = query_embedding else {
        return 0.0;
    };
    if row_dimension != Some(query_embedding.dimension)
        || row_model != Some(query_embedding.model.as_str())
        || row_version != Some(query_embedding.version.as_str())
    {
        return 0.0;
    }
    row_embedding
        .map(|embedding| cosine_similarity(&query_embedding.vector, embedding))
        .unwrap_or(0.0)
}

fn keyword_score(query_tokens: &BTreeSet<String>, text: &str) -> f64 {
    if query_tokens.is_empty() {
        return 0.0;
    }
    let text_tokens = token_set(text);
    let hits = query_tokens
        .iter()
        .filter(|token| text_tokens.contains(*token))
        .count();
    if hits == 0 {
        0.0
    } else {
        hits as f64 / query_tokens.len() as f64
    }
}

fn token_set(text: &str) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    let mut current = String::new();
    for character in text.chars().flat_map(char::to_lowercase) {
        if character.is_alphanumeric() || character == '_' || character == '-' {
            current.push(character);
        } else if !current.is_empty() {
            tokens.insert(current.clone());
            current.clear();
        }
    }
    if !current.is_empty() {
        tokens.insert(current);
    }
    tokens
}

fn parse_string_array(value: &str, field: &'static str) -> Result<Vec<String>, CommandError> {
    serde_json::from_str(value).map_err(|error| {
        CommandError::system_fault(
            "agent_retrieval_metadata_decode_failed",
            format!("Xero could not decode retrieval field {field}: {error}"),
        )
    })
}

fn embedding_is_current(
    embedding: Option<&Vec<f32>>,
    model: Option<&str>,
    dimension: Option<i32>,
    version: Option<&str>,
) -> bool {
    embedding.is_some()
        && model == Some(DEFAULT_AGENT_EMBEDDING_MODEL)
        && dimension == Some(super::agent_embeddings::AGENT_RETRIEVAL_EMBEDDING_DIM)
        && version == Some(DEFAULT_AGENT_EMBEDDING_VERSION)
}

fn freshness_score_adjustment(freshness_state: &str) -> f64 {
    match freshness_state {
        "current" => 0.20,
        "source_unknown" => 0.0,
        "stale" => -0.15,
        "source_missing" => -0.25,
        "superseded" => -0.30,
        _ => 0.0,
    }
}

fn embedding_backfill_job_id(
    source_kind: AgentEmbeddingBackfillSourceKind,
    source_id: &str,
    source_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(backfill_source_kind_sql_value(&source_kind).as_bytes());
    hasher.update(b"\0");
    hasher.update(source_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(source_hash.as_bytes());
    hasher.update(b"\0");
    hasher.update(DEFAULT_AGENT_EMBEDDING_MODEL.as_bytes());
    hasher.update(b"\0");
    hasher.update(DEFAULT_AGENT_EMBEDDING_VERSION.as_bytes());
    let hash = format!("{:x}", hasher.finalize());
    format!("embedding-backfill-{}", &hash[..16])
}

fn get_agent_embedding_backfill_job(
    repo_root: &Path,
    project_id: &str,
    source_kind: &AgentEmbeddingBackfillSourceKind,
    source_id: &str,
    embedding_model: &str,
    embedding_version: &str,
) -> Result<Option<AgentEmbeddingBackfillJobRecord>, CommandError> {
    let connection = open_backfill_database(repo_root)?;
    connection
        .query_row(
            backfill_job_select_sql(
                r#"
                WHERE project_id = ?1
                  AND source_kind = ?2
                  AND source_id = ?3
                  AND embedding_model = ?4
                  AND embedding_version = ?5
                "#,
            )
            .as_str(),
            params![
                project_id,
                backfill_source_kind_sql_value(source_kind),
                source_id,
                embedding_model,
                embedding_version
            ],
            read_backfill_job_row,
        )
        .optional()
        .map_err(map_backfill_read_error)?
        .transpose()
}

fn list_pending_agent_embedding_backfill_jobs(
    repo_root: &Path,
    project_id: &str,
    limit: u32,
) -> Result<Vec<AgentEmbeddingBackfillJobRecord>, CommandError> {
    let connection = open_backfill_database(repo_root)?;
    let sql = format!(
        "{} LIMIT ?2",
        backfill_job_select_sql(
            "WHERE project_id = ?1 AND status = 'pending' ORDER BY created_at ASC, id ASC",
        )
    );
    let mut statement = connection.prepare(&sql).map_err(map_backfill_read_error)?;
    let rows = statement
        .query_map(params![project_id, limit], read_backfill_job_row)
        .map_err(map_backfill_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_backfill_read_error)?
        .into_iter()
        .collect()
}

#[expect(
    clippy::too_many_arguments,
    reason = "Backfill status updates mirror the persisted job columns to keep call sites explicit."
)]
fn update_backfill_job_status(
    repo_root: &Path,
    project_id: &str,
    job_id: &str,
    status: AgentEmbeddingBackfillStatus,
    attempts: u32,
    diagnostic: Option<JsonValue>,
    updated_at: &str,
    completed_at: Option<&str>,
) -> Result<(), CommandError> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let diagnostic_json = diagnostic
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "agent_embedding_backfill_diagnostic_serialize_failed",
                format!("Xero could not serialize embedding backfill diagnostic: {error}"),
            )
        })?;
    connection
        .execute(
            r#"
            UPDATE agent_embedding_backfill_jobs
            SET status = ?3,
                attempts = ?4,
                diagnostic_json = ?5,
                updated_at = ?6,
                completed_at = ?7
            WHERE project_id = ?1
              AND job_id = ?2
            "#,
            params![
                project_id,
                job_id,
                backfill_status_sql_value(&status),
                attempts,
                diagnostic_json,
                updated_at,
                completed_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "agent_embedding_backfill_job_update_failed",
                format!(
                    "Xero could not update an embedding backfill job in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

fn open_backfill_database(repo_root: &Path) -> Result<rusqlite::Connection, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    open_runtime_database(repo_root, &database_path)
}

fn backfill_job_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            job_id,
            project_id,
            source_kind,
            source_id,
            source_hash,
            embedding_model,
            embedding_dimension,
            embedding_version,
            status,
            attempts,
            diagnostic_json,
            created_at,
            updated_at,
            completed_at
        FROM agent_embedding_backfill_jobs
        {where_clause}
        "#
    )
}

fn read_backfill_job_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<AgentEmbeddingBackfillJobRecord, CommandError>> {
    let diagnostic_json: Option<String> = row.get(11)?;
    Ok(Ok(AgentEmbeddingBackfillJobRecord {
        id: row.get(0)?,
        job_id: row.get(1)?,
        project_id: row.get(2)?,
        source_kind: parse_backfill_source_kind(row.get::<_, String>(3)?.as_str()),
        source_id: row.get(4)?,
        source_hash: row.get(5)?,
        embedding_model: row.get(6)?,
        embedding_dimension: row.get(7)?,
        embedding_version: row.get(8)?,
        status: parse_backfill_status(row.get::<_, String>(9)?.as_str()),
        attempts: row.get::<_, i64>(10)?.max(0) as u32,
        diagnostic: diagnostic_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
        completed_at: row.get(14)?,
    }))
}

fn map_backfill_read_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "agent_embedding_backfill_read_failed",
        format!("Xero could not read embedding backfill state: {error}"),
    )
}

fn backfill_source_kind_sql_value(kind: &AgentEmbeddingBackfillSourceKind) -> &'static str {
    match kind {
        AgentEmbeddingBackfillSourceKind::ProjectRecord => "project_record",
        AgentEmbeddingBackfillSourceKind::ApprovedMemory => "approved_memory",
    }
}

fn parse_backfill_source_kind(value: &str) -> AgentEmbeddingBackfillSourceKind {
    match value {
        "approved_memory" => AgentEmbeddingBackfillSourceKind::ApprovedMemory,
        _ => AgentEmbeddingBackfillSourceKind::ProjectRecord,
    }
}

fn backfill_status_sql_value(status: &AgentEmbeddingBackfillStatus) -> &'static str {
    match status {
        AgentEmbeddingBackfillStatus::Pending => "pending",
        AgentEmbeddingBackfillStatus::Running => "running",
        AgentEmbeddingBackfillStatus::Succeeded => "succeeded",
        AgentEmbeddingBackfillStatus::Failed => "failed",
        AgentEmbeddingBackfillStatus::Skipped => "skipped",
    }
}

fn parse_backfill_status(value: &str) -> AgentEmbeddingBackfillStatus {
    match value {
        "running" => AgentEmbeddingBackfillStatus::Running,
        "succeeded" => AgentEmbeddingBackfillStatus::Succeeded,
        "failed" => AgentEmbeddingBackfillStatus::Failed,
        "skipped" => AgentEmbeddingBackfillStatus::Skipped,
        _ => AgentEmbeddingBackfillStatus::Pending,
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

fn project_record_redaction_state_value(
    redaction_state: &ProjectRecordRedactionState,
) -> &'static str {
    match redaction_state {
        ProjectRecordRedactionState::Clean => "clean",
        ProjectRecordRedactionState::Redacted => "redacted",
        ProjectRecordRedactionState::Blocked => "blocked",
    }
}

fn project_importance_rank(importance: &ProjectRecordImportance) -> u8 {
    match importance {
        ProjectRecordImportance::Low => 0,
        ProjectRecordImportance::Normal => 1,
        ProjectRecordImportance::High => 2,
        ProjectRecordImportance::Critical => 3,
    }
}

fn importance_rank(value: &str) -> u8 {
    match value {
        "high" => 2,
        "critical" => 3,
        "low" => 0,
        _ => 1,
    }
}

fn memory_scope_sql_value(scope: &AgentMemoryScope) -> &'static str {
    match scope {
        AgentMemoryScope::Project => "project",
        AgentMemoryScope::Session => "session",
    }
}

fn memory_kind_sql_value(kind: &AgentMemoryKind) -> &'static str {
    match kind {
        AgentMemoryKind::ProjectFact => "project_fact",
        AgentMemoryKind::UserPreference => "user_preference",
        AgentMemoryKind::Decision => "decision",
        AgentMemoryKind::SessionSummary => "session_summary",
        AgentMemoryKind::Troubleshooting => "troubleshooting",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyword_score_requires_text_overlap() {
        let query = token_set("lancedb memory");
        assert!(keyword_score(&query, "LanceDB stores reviewed memory.") > 0.0);
        assert_eq!(keyword_score(&query, "unrelated words"), 0.0);
    }

    #[test]
    fn backfill_job_ids_are_deterministic() {
        let first = embedding_backfill_job_id(
            AgentEmbeddingBackfillSourceKind::ProjectRecord,
            "record-1",
            &"a".repeat(64),
        );
        let second = embedding_backfill_job_id(
            AgentEmbeddingBackfillSourceKind::ProjectRecord,
            "record-1",
            &"a".repeat(64),
        );
        assert_eq!(first, second);
        assert!(first.starts_with("embedding-backfill-"));
    }
}
