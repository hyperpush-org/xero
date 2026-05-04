//! Lance-backed storage for agent memories.
//!
//! Holds the per-project Lance dataset under
//! `<app-data>/projects/<project-id>/lance/agent_memories.lance/`. The public
//! API stays synchronous for the surrounding Tauri command layer by wrapping
//! async LanceDB operations with a dedicated tokio runtime.
//!
//! Lance lives outside the SQLite transaction boundary, so the store does not
//! enforce relational invariants (uniqueness, FK cascades) at the storage
//! layer. The callers — `agent_memory.rs` and `agent_session.rs` — are
//! responsible for application-level dedup checks (`find_active_memory_by_hash`)
//! and cascade clearing when sessions/runs are deleted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use arrow_array::builder::{
    BooleanBuilder, FixedSizeListBuilder, Float32Builder, Int32Builder, StringBuilder, UInt8Builder,
};
use arrow_array::{
    Array, ArrayRef, BooleanArray, FixedSizeListArray, Int32Array, RecordBatch,
    RecordBatchIterator, StringArray, UInt8Array,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use lancedb::query::ExecutableQuery;
use lancedb::{Connection, Table};
use tokio::runtime::Runtime;

use crate::commands::CommandError;

use super::agent_core::AgentRunDiagnosticRecord;
use super::agent_embeddings::AGENT_RETRIEVAL_EMBEDDING_DIM;
use super::agent_memory::{
    AgentMemoryKind, AgentMemoryRecord, AgentMemoryReviewState, AgentMemoryScope,
};
use super::{FreshnessUpdate, SupersessionUpdate};

/// Reserved fixed dimension for opt-in semantic embeddings. Picked to match the
/// most common embedding sizes (768 = MiniLM/E5; OpenAI text-embedding-3-small
/// truncation default). Embedding writes are not yet wired; the column is
/// declared so future code can populate it without a schema migration.
pub const AGENT_MEMORY_EMBEDDING_DIM: i32 = AGENT_RETRIEVAL_EMBEDDING_DIM;

/// Lance-table identifier inside the dataset directory. LanceDB datasets use
/// directories named `<table>.lance/`, but the connection API expects the
/// table name without the `.lance` suffix.
const AGENT_MEMORIES_TABLE: &str = "agent_memories";

/// Subdirectory under each per-project app-data dir that hosts Lance datasets.
pub const PROJECT_LANCE_SUBDIR: &str = "lance";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AgentMemoryListFilterOwned {
    pub include_disabled: bool,
    pub include_rejected: bool,
}

#[derive(Debug, Clone)]
pub struct AgentMemoryUpdate {
    /// Logical owner of the memory. Currently only used for diagnostics —
    /// dataset-level scoping is enforced by the per-project Lance directory.
    #[allow(dead_code)]
    pub project_id: String,
    pub memory_id: String,
    pub review_state: Option<AgentMemoryReviewState>,
    pub enabled: Option<bool>,
    pub diagnostic: Option<AgentRunDiagnosticRecord>,
}

pub fn dataset_dir_for_database_path(database_path: &Path) -> PathBuf {
    database_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(PROJECT_LANCE_SUBDIR)
}

pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("memory_id", DataType::Utf8, false),
        Field::new("agent_session_id", DataType::Utf8, true),
        Field::new("scope_kind", DataType::Utf8, false),
        Field::new("memory_kind", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("text_hash", DataType::Utf8, false),
        Field::new("review_state", DataType::Utf8, false),
        Field::new("enabled", DataType::Boolean, false),
        Field::new("confidence", DataType::UInt8, true),
        Field::new("source_run_id", DataType::Utf8, true),
        Field::new("source_item_ids_json", DataType::Utf8, false),
        Field::new("diagnostic_json", DataType::Utf8, true),
        Field::new("freshness_state", DataType::Utf8, false),
        Field::new("freshness_checked_at", DataType::Utf8, true),
        Field::new("stale_reason", DataType::Utf8, true),
        Field::new("source_fingerprints_json", DataType::Utf8, false),
        Field::new("supersedes_id", DataType::Utf8, true),
        Field::new("superseded_by_id", DataType::Utf8, true),
        Field::new("invalidated_at", DataType::Utf8, true),
        Field::new("fact_key", DataType::Utf8, true),
        Field::new("created_at", DataType::Utf8, false),
        Field::new("updated_at", DataType::Utf8, false),
        Field::new("embedding_model", DataType::Utf8, true),
        Field::new("embedding_dimension", DataType::Int32, true),
        Field::new("embedding_version", DataType::Utf8, true),
        Field::new(
            "embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                AGENT_MEMORY_EMBEDDING_DIM,
            ),
            true,
        ),
    ]))
}

/// Logical row form used by both the live store and the migration importer.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentMemoryRow {
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
    pub embedding: Option<Vec<f32>>,
    pub embedding_model: Option<String>,
    pub embedding_dimension: Option<i32>,
    pub embedding_version: Option<String>,
}

impl AgentMemoryRow {
    pub fn into_record(self) -> AgentMemoryRecord {
        AgentMemoryRecord {
            id: 0,
            memory_id: self.memory_id,
            project_id: self.project_id,
            agent_session_id: self.agent_session_id,
            scope: self.scope,
            kind: self.kind,
            text: self.text,
            text_hash: self.text_hash,
            review_state: self.review_state,
            enabled: self.enabled,
            confidence: self.confidence,
            source_run_id: self.source_run_id,
            source_item_ids: self.source_item_ids,
            diagnostic: self.diagnostic,
            freshness_state: self.freshness_state,
            freshness_checked_at: self.freshness_checked_at,
            stale_reason: self.stale_reason,
            source_fingerprints_json: self.source_fingerprints_json,
            supersedes_id: self.supersedes_id,
            superseded_by_id: self.superseded_by_id,
            invalidated_at: self.invalidated_at,
            fact_key: self.fact_key,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// Dedicated tokio runtime that owns all lancedb async work. Lance pulls in
/// tokio under the hood; sharing a single runtime avoids the cost of spawning
/// a fresh one per call and matches the pattern used by `idb_client.rs`.
pub(crate) fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("xero-lance")
            .enable_all()
            .build()
            .expect("xero lance tokio runtime")
    })
}

#[derive(Default)]
struct ConnectionCache {
    inner: HashMap<PathBuf, Connection>,
}

fn connection_cache() -> &'static Mutex<ConnectionCache> {
    static CACHE: LazyLock<Mutex<ConnectionCache>> =
        LazyLock::new(|| Mutex::new(ConnectionCache::default()));
    &CACHE
}

/// Resets the in-process connection cache. Tests use this when tearing down a
/// tempdir so that subsequent `connect()` calls hit a fresh disk layout.
#[cfg(test)]
pub fn reset_connection_cache_for_tests() {
    if let Ok(mut cache) = connection_cache().lock() {
        cache.inner.clear();
    }
}

fn map_lance_error<E: std::fmt::Display>(code: &'static str, error: E) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero agent_memory lance store failed: {error}"),
    )
}

async fn connect_dataset(dataset_dir: &Path) -> Result<Connection, CommandError> {
    std::fs::create_dir_all(dataset_dir).map_err(|error| {
        CommandError::retryable(
            "agent_memory_lance_dir_unavailable",
            format!(
                "Xero could not prepare the lance dataset directory at {}: {error}",
                dataset_dir.display()
            ),
        )
    })?;
    let uri = dataset_dir
        .to_str()
        .ok_or_else(|| {
            CommandError::system_fault(
                "agent_memory_lance_dir_non_utf8",
                format!(
                    "Xero cannot use non-UTF8 lance dataset path {}.",
                    dataset_dir.display()
                ),
            )
        })?
        .to_string();
    lancedb::connect(&uri)
        .execute()
        .await
        .map_err(|error| map_lance_error("agent_memory_lance_connect_failed", error))
}

async fn ensure_connection(dataset_dir: &Path) -> Result<Connection, CommandError> {
    {
        let cache = connection_cache().lock().expect("lance cache poisoned");
        if let Some(connection) = cache.inner.get(dataset_dir).cloned() {
            return Ok(connection);
        }
    }
    let connection = connect_dataset(dataset_dir).await?;
    let mut cache = connection_cache().lock().expect("lance cache poisoned");
    cache
        .inner
        .insert(dataset_dir.to_path_buf(), connection.clone());
    Ok(connection)
}

async fn open_or_create_table(connection: &Connection) -> Result<Table, CommandError> {
    match connection.open_table(AGENT_MEMORIES_TABLE).execute().await {
        Ok(table) => Ok(table),
        Err(_) => {
            let schema = schema();
            let empty = RecordBatch::new_empty(schema.clone());
            let iter = RecordBatchIterator::new(
                std::iter::once(Ok::<_, arrow_schema::ArrowError>(empty)),
                schema,
            );
            let reader: Box<dyn arrow_array::RecordBatchReader + Send + 'static> = Box::new(iter);
            connection
                .create_table(AGENT_MEMORIES_TABLE, reader)
                .execute()
                .await
                .map_err(|error| map_lance_error("agent_memory_lance_create_table_failed", error))
        }
    }
}

fn build_batch(rows: &[AgentMemoryRow]) -> Result<RecordBatch, CommandError> {
    let schema = schema();
    let mut memory_id = StringBuilder::new();
    let mut agent_session_id = StringBuilder::new();
    let mut scope_kind = StringBuilder::new();
    let mut memory_kind = StringBuilder::new();
    let mut text = StringBuilder::new();
    let mut text_hash = StringBuilder::new();
    let mut review_state = StringBuilder::new();
    let mut enabled = BooleanBuilder::new();
    let mut confidence = UInt8Builder::new();
    let mut source_run_id = StringBuilder::new();
    let mut source_item_ids_json = StringBuilder::new();
    let mut diagnostic_json = StringBuilder::new();
    let mut freshness_state = StringBuilder::new();
    let mut freshness_checked_at = StringBuilder::new();
    let mut stale_reason = StringBuilder::new();
    let mut source_fingerprints_json = StringBuilder::new();
    let mut supersedes_id = StringBuilder::new();
    let mut superseded_by_id = StringBuilder::new();
    let mut invalidated_at = StringBuilder::new();
    let mut fact_key = StringBuilder::new();
    let mut created_at = StringBuilder::new();
    let mut updated_at = StringBuilder::new();
    let mut embedding_model = StringBuilder::new();
    let mut embedding_dimension = Int32Builder::new();
    let mut embedding_version = StringBuilder::new();
    let mut embedding =
        FixedSizeListBuilder::new(Float32Builder::new(), AGENT_MEMORY_EMBEDDING_DIM);

    for row in rows {
        memory_id.append_value(&row.memory_id);
        match &row.agent_session_id {
            Some(value) => agent_session_id.append_value(value),
            None => agent_session_id.append_null(),
        }
        scope_kind.append_value(scope_sql_value(&row.scope));
        memory_kind.append_value(kind_sql_value(&row.kind));
        text.append_value(&row.text);
        text_hash.append_value(&row.text_hash);
        review_state.append_value(review_state_sql_value(&row.review_state));
        enabled.append_value(row.enabled);
        match row.confidence {
            Some(value) => confidence.append_value(value),
            None => confidence.append_null(),
        }
        match &row.source_run_id {
            Some(value) => source_run_id.append_value(value),
            None => source_run_id.append_null(),
        }
        let items_json = serde_json::to_string(&row.source_item_ids).map_err(|error| {
            CommandError::system_fault(
                "agent_memory_lance_serialize_failed",
                format!(
                    "Xero could not serialize memory source item ids for `{}`: {error}",
                    row.memory_id
                ),
            )
        })?;
        source_item_ids_json.append_value(&items_json);
        match &row.diagnostic {
            Some(diagnostic) => {
                let json = serde_json::to_string(&serde_json::json!({
                    "code": diagnostic.code,
                    "message": diagnostic.message,
                }))
                .map_err(|error| {
                    CommandError::system_fault(
                        "agent_memory_lance_serialize_failed",
                        format!(
                            "Xero could not serialize memory diagnostic for `{}`: {error}",
                            row.memory_id
                        ),
                    )
                })?;
                diagnostic_json.append_value(&json);
            }
            None => diagnostic_json.append_null(),
        }
        freshness_state.append_value(&row.freshness_state);
        append_optional(
            &mut freshness_checked_at,
            row.freshness_checked_at.as_deref(),
        );
        append_optional(&mut stale_reason, row.stale_reason.as_deref());
        source_fingerprints_json.append_value(&row.source_fingerprints_json);
        append_optional(&mut supersedes_id, row.supersedes_id.as_deref());
        append_optional(&mut superseded_by_id, row.superseded_by_id.as_deref());
        append_optional(&mut invalidated_at, row.invalidated_at.as_deref());
        append_optional(&mut fact_key, row.fact_key.as_deref());
        created_at.append_value(&row.created_at);
        updated_at.append_value(&row.updated_at);
        match &row.embedding_model {
            Some(value) => embedding_model.append_value(value),
            None => embedding_model.append_null(),
        }
        match row.embedding_dimension {
            Some(value) => embedding_dimension.append_value(value),
            None => embedding_dimension.append_null(),
        }
        match &row.embedding_version {
            Some(value) => embedding_version.append_value(value),
            None => embedding_version.append_null(),
        }
        append_embedding(&mut embedding, row.embedding.as_deref())?;
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(memory_id.finish()),
        Arc::new(agent_session_id.finish()),
        Arc::new(scope_kind.finish()),
        Arc::new(memory_kind.finish()),
        Arc::new(text.finish()),
        Arc::new(text_hash.finish()),
        Arc::new(review_state.finish()),
        Arc::new(enabled.finish()),
        Arc::new(confidence.finish()),
        Arc::new(source_run_id.finish()),
        Arc::new(source_item_ids_json.finish()),
        Arc::new(diagnostic_json.finish()),
        Arc::new(freshness_state.finish()),
        Arc::new(freshness_checked_at.finish()),
        Arc::new(stale_reason.finish()),
        Arc::new(source_fingerprints_json.finish()),
        Arc::new(supersedes_id.finish()),
        Arc::new(superseded_by_id.finish()),
        Arc::new(invalidated_at.finish()),
        Arc::new(fact_key.finish()),
        Arc::new(created_at.finish()),
        Arc::new(updated_at.finish()),
        Arc::new(embedding_model.finish()),
        Arc::new(embedding_dimension.finish()),
        Arc::new(embedding_version.finish()),
        Arc::new(embedding.finish()),
    ];
    RecordBatch::try_new(schema, columns).map_err(|error| {
        CommandError::system_fault(
            "agent_memory_lance_record_batch_failed",
            format!("Xero could not assemble lance record batch: {error}"),
        )
    })
}

fn append_optional(builder: &mut StringBuilder, value: Option<&str>) {
    match value {
        Some(value) => builder.append_value(value),
        None => builder.append_null(),
    }
}

fn batches_to_rows(batches: Vec<RecordBatch>) -> Result<Vec<AgentMemoryRow>, CommandError> {
    let mut rows = Vec::new();
    for batch in batches {
        rows.extend(batch_to_rows(&batch)?);
    }
    Ok(rows)
}

fn append_embedding(
    builder: &mut FixedSizeListBuilder<Float32Builder>,
    embedding: Option<&[f32]>,
) -> Result<(), CommandError> {
    match embedding {
        Some(values) if values.len() == AGENT_MEMORY_EMBEDDING_DIM as usize => {
            for value in values {
                builder.values().append_value(*value);
            }
            builder.append(true);
            Ok(())
        }
        Some(values) => Err(CommandError::system_fault(
            "agent_memory_lance_embedding_dimension_mismatch",
            format!(
                "Xero agent-memory embedding has {} dimensions; expected {}.",
                values.len(),
                AGENT_MEMORY_EMBEDDING_DIM
            ),
        )),
        None => {
            for _ in 0..AGENT_MEMORY_EMBEDDING_DIM {
                builder.values().append_null();
            }
            builder.append(false);
            Ok(())
        }
    }
}

fn batch_to_rows(batch: &RecordBatch) -> Result<Vec<AgentMemoryRow>, CommandError> {
    let row_count = batch.num_rows();
    if row_count == 0 {
        return Ok(Vec::new());
    }

    let memory_id_arr = column_str(batch, "memory_id")?;
    let agent_session_id_arr = column_str(batch, "agent_session_id")?;
    let scope_kind_arr = column_str(batch, "scope_kind")?;
    let memory_kind_arr = column_str(batch, "memory_kind")?;
    let text_arr = column_str(batch, "text")?;
    let text_hash_arr = column_str(batch, "text_hash")?;
    let review_state_arr = column_str(batch, "review_state")?;
    let enabled_arr = column_bool(batch, "enabled")?;
    let confidence_arr = column_u8(batch, "confidence")?;
    let source_run_id_arr = column_str(batch, "source_run_id")?;
    let source_item_ids_json_arr = column_str(batch, "source_item_ids_json")?;
    let diagnostic_json_arr = column_str(batch, "diagnostic_json")?;
    let freshness_state_arr = column_str(batch, "freshness_state")?;
    let freshness_checked_at_arr = column_str(batch, "freshness_checked_at")?;
    let stale_reason_arr = column_str(batch, "stale_reason")?;
    let source_fingerprints_json_arr = column_str(batch, "source_fingerprints_json")?;
    let supersedes_id_arr = column_str(batch, "supersedes_id")?;
    let superseded_by_id_arr = column_str(batch, "superseded_by_id")?;
    let invalidated_at_arr = column_str(batch, "invalidated_at")?;
    let fact_key_arr = column_str(batch, "fact_key")?;
    let created_at_arr = column_str(batch, "created_at")?;
    let updated_at_arr = column_str(batch, "updated_at")?;
    let embedding_model_arr = column_str(batch, "embedding_model")?;
    let embedding_dimension_arr = column_i32(batch, "embedding_dimension")?;
    let embedding_version_arr = column_str(batch, "embedding_version")?;
    let embedding_arr = column_embedding(batch, "embedding")?;

    let mut rows = Vec::with_capacity(row_count);
    for index in 0..row_count {
        let memory_id = require_str(memory_id_arr, index, "memory_id")?;
        let scope = parse_scope(require_str(scope_kind_arr, index, "scope_kind")?);
        let kind = parse_kind(require_str(memory_kind_arr, index, "memory_kind")?);
        let review_state =
            parse_review_state(require_str(review_state_arr, index, "review_state")?);
        let source_item_ids = decode_source_item_ids(require_str(
            source_item_ids_json_arr,
            index,
            "source_item_ids_json",
        )?)?;
        let diagnostic = if diagnostic_json_arr.is_null(index) {
            None
        } else {
            Some(decode_diagnostic(diagnostic_json_arr.value(index))?)
        };
        rows.push(AgentMemoryRow {
            memory_id: memory_id.to_string(),
            project_id: String::new(),
            agent_session_id: optional_str(agent_session_id_arr, index),
            scope,
            kind,
            text: require_str(text_arr, index, "text")?.to_string(),
            text_hash: require_str(text_hash_arr, index, "text_hash")?.to_string(),
            review_state,
            enabled: enabled_arr.value(index),
            confidence: if confidence_arr.is_null(index) {
                None
            } else {
                Some(confidence_arr.value(index))
            },
            source_run_id: optional_str(source_run_id_arr, index),
            source_item_ids,
            diagnostic,
            freshness_state: require_str(freshness_state_arr, index, "freshness_state")?
                .to_string(),
            freshness_checked_at: optional_str(freshness_checked_at_arr, index),
            stale_reason: optional_str(stale_reason_arr, index),
            source_fingerprints_json: require_str(
                source_fingerprints_json_arr,
                index,
                "source_fingerprints_json",
            )?
            .to_string(),
            supersedes_id: optional_str(supersedes_id_arr, index),
            superseded_by_id: optional_str(superseded_by_id_arr, index),
            invalidated_at: optional_str(invalidated_at_arr, index),
            fact_key: optional_str(fact_key_arr, index),
            created_at: require_str(created_at_arr, index, "created_at")?.to_string(),
            updated_at: require_str(updated_at_arr, index, "updated_at")?.to_string(),
            embedding: optional_embedding(embedding_arr, index)?,
            embedding_model: optional_str(embedding_model_arr, index),
            embedding_dimension: if embedding_dimension_arr.is_null(index) {
                None
            } else {
                Some(embedding_dimension_arr.value(index))
            },
            embedding_version: optional_str(embedding_version_arr, index),
        });
    }
    Ok(rows)
}

fn column_str<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a StringArray, CommandError> {
    batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<StringArray>())
        .ok_or_else(|| missing_column(name))
}

fn column_bool<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a BooleanArray, CommandError> {
    batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<BooleanArray>())
        .ok_or_else(|| missing_column(name))
}

fn column_u8<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a UInt8Array, CommandError> {
    batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<UInt8Array>())
        .ok_or_else(|| missing_column(name))
}

fn column_i32<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Int32Array, CommandError> {
    batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<Int32Array>())
        .ok_or_else(|| missing_column(name))
}

fn column_embedding<'a>(
    batch: &'a RecordBatch,
    name: &str,
) -> Result<&'a FixedSizeListArray, CommandError> {
    batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<FixedSizeListArray>())
        .ok_or_else(|| missing_column(name))
}

fn missing_column(name: &str) -> CommandError {
    CommandError::system_fault(
        "agent_memory_lance_schema_drift",
        format!("Xero lance dataset is missing expected column `{name}`."),
    )
}

fn require_str<'a>(
    array: &'a StringArray,
    index: usize,
    column: &str,
) -> Result<&'a str, CommandError> {
    if array.is_null(index) {
        return Err(CommandError::system_fault(
            "agent_memory_lance_unexpected_null",
            format!("Xero lance dataset has unexpected null in column `{column}`."),
        ));
    }
    Ok(array.value(index))
}

fn optional_str(array: &StringArray, index: usize) -> Option<String> {
    if array.is_null(index) {
        None
    } else {
        Some(array.value(index).to_string())
    }
}

fn optional_embedding(
    array: &FixedSizeListArray,
    index: usize,
) -> Result<Option<Vec<f32>>, CommandError> {
    if array.is_null(index) {
        return Ok(None);
    }
    if array.value_length() != AGENT_MEMORY_EMBEDDING_DIM {
        return Err(CommandError::system_fault(
            "agent_memory_lance_embedding_dimension_mismatch",
            format!(
                "Xero agent-memory Lance embedding dimension is {}; expected {}.",
                array.value_length(),
                AGENT_MEMORY_EMBEDDING_DIM
            ),
        ));
    }
    let values = array.value(index);
    let values = values
        .as_any()
        .downcast_ref::<arrow_array::Float32Array>()
        .ok_or_else(|| missing_column("embedding.item"))?;
    let mut vector = Vec::with_capacity(AGENT_MEMORY_EMBEDDING_DIM as usize);
    for value_index in 0..AGENT_MEMORY_EMBEDDING_DIM as usize {
        if values.is_null(value_index) {
            return Ok(None);
        }
        vector.push(values.value(value_index));
    }
    Ok(Some(vector))
}

fn decode_source_item_ids(value: &str) -> Result<Vec<String>, CommandError> {
    serde_json::from_str(value).map_err(|error| {
        CommandError::system_fault(
            "agent_memory_lance_decode_failed",
            format!("Xero could not decode lance source_item_ids_json: {error}"),
        )
    })
}

fn decode_diagnostic(value: &str) -> Result<AgentRunDiagnosticRecord, CommandError> {
    let parsed: serde_json::Value = serde_json::from_str(value).map_err(|error| {
        CommandError::system_fault(
            "agent_memory_lance_decode_failed",
            format!("Xero could not decode lance diagnostic_json: {error}"),
        )
    })?;
    Ok(AgentRunDiagnosticRecord {
        code: parsed
            .get("code")
            .and_then(|value| value.as_str())
            .unwrap_or("agent_memory_diagnostic")
            .to_string(),
        message: parsed
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("Xero could not decode memory diagnostic details.")
            .to_string(),
    })
}

fn scope_sql_value(scope: &AgentMemoryScope) -> &'static str {
    match scope {
        AgentMemoryScope::Project => "project",
        AgentMemoryScope::Session => "session",
    }
}

fn parse_scope(value: &str) -> AgentMemoryScope {
    match value {
        "session" => AgentMemoryScope::Session,
        _ => AgentMemoryScope::Project,
    }
}

fn kind_sql_value(kind: &AgentMemoryKind) -> &'static str {
    match kind {
        AgentMemoryKind::ProjectFact => "project_fact",
        AgentMemoryKind::UserPreference => "user_preference",
        AgentMemoryKind::Decision => "decision",
        AgentMemoryKind::SessionSummary => "session_summary",
        AgentMemoryKind::Troubleshooting => "troubleshooting",
    }
}

fn parse_kind(value: &str) -> AgentMemoryKind {
    match value {
        "user_preference" => AgentMemoryKind::UserPreference,
        "decision" => AgentMemoryKind::Decision,
        "session_summary" => AgentMemoryKind::SessionSummary,
        "troubleshooting" => AgentMemoryKind::Troubleshooting,
        _ => AgentMemoryKind::ProjectFact,
    }
}

fn review_state_sql_value(review_state: &AgentMemoryReviewState) -> &'static str {
    match review_state {
        AgentMemoryReviewState::Candidate => "candidate",
        AgentMemoryReviewState::Approved => "approved",
        AgentMemoryReviewState::Rejected => "rejected",
    }
}

fn parse_review_state(value: &str) -> AgentMemoryReviewState {
    match value {
        "approved" => AgentMemoryReviewState::Approved,
        "rejected" => AgentMemoryReviewState::Rejected,
        _ => AgentMemoryReviewState::Candidate,
    }
}

/// Project-scoped store handle returned by [`open_for_database_path`]. Holds
/// the dataset path so call sites can compose subsequent ops without
/// re-resolving the filesystem layout.
pub struct ProjectMemoryStore {
    project_id: String,
    dataset_dir: PathBuf,
}

pub fn open_for_database_path(database_path: &Path, project_id: &str) -> ProjectMemoryStore {
    ProjectMemoryStore {
        project_id: project_id.to_string(),
        dataset_dir: dataset_dir_for_database_path(database_path),
    }
}

impl ProjectMemoryStore {
    #[allow(dead_code)]
    pub fn dataset_dir(&self) -> &Path {
        &self.dataset_dir
    }

    #[allow(dead_code)]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    pub fn insert(&self, mut row: AgentMemoryRow) -> Result<AgentMemoryRecord, CommandError> {
        row.project_id = self.project_id.clone();
        row.updated_at = row.created_at.clone();
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let result = runtime().block_on(async move {
            let rows = scan_all(&dataset).await?;
            if let Some(existing) = rows.iter().find(|existing| same_dedup_key(existing, &row)) {
                return Ok::<AgentMemoryRow, CommandError>(existing.clone());
            }
            let connection = ensure_connection(&dataset).await?;
            let table = open_or_create_table(&connection).await?;
            insert_row(&table, &row).await?;
            Ok::<AgentMemoryRow, CommandError>(row)
        })?;
        // Return the canonical stored shape so callers see the project_id we
        // stamped on the way in even if they passed a different one.
        let mut record = result.into_record();
        record.project_id = project_id;
        Ok(record)
    }

    pub(crate) fn list_rows(
        &self,
        agent_session_id: Option<&str>,
        filter: AgentMemoryListFilterOwned,
    ) -> Result<Vec<AgentMemoryRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let agent_session_id = agent_session_id.map(|value| value.to_string());
        runtime().block_on(async move {
            let rows = scan_all(&dataset).await?;
            Ok(filter_rows(rows, &agent_session_id, filter)
                .into_iter()
                .map(|row| stamp_project(row, &project_id))
                .collect())
        })
    }

    pub(crate) fn list_all_rows(&self) -> Result<Vec<AgentMemoryRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        runtime().block_on(async move {
            let rows = scan_all(&dataset).await?;
            Ok(rows
                .into_iter()
                .map(|row| stamp_project(row, &project_id))
                .collect())
        })
    }

    pub fn list(
        &self,
        agent_session_id: Option<&str>,
        filter: AgentMemoryListFilterOwned,
    ) -> Result<Vec<AgentMemoryRecord>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let agent_session_id = agent_session_id.map(|value| value.to_string());
        runtime().block_on(async move {
            let rows = scan_all(&dataset).await?;
            let filtered = filter_rows(rows, &agent_session_id, filter);
            Ok(into_records(filtered, &project_id, ordering_for_list))
        })
    }

    pub fn list_approved(
        &self,
        agent_session_id: Option<&str>,
    ) -> Result<Vec<AgentMemoryRecord>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let agent_session_id = agent_session_id.map(|value| value.to_string());
        runtime().block_on(async move {
            let rows = scan_all(&dataset).await?;
            let approved = filter_approved(rows, &agent_session_id);
            Ok(into_records(
                approved,
                &project_id,
                ordering_for_list_approved,
            ))
        })
    }

    pub fn find_active_by_hash(
        &self,
        scope: &AgentMemoryScope,
        agent_session_id: Option<&str>,
        kind: &AgentMemoryKind,
        text_hash: &str,
    ) -> Result<Option<AgentMemoryRecord>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let scope_value = scope_sql_value(scope).to_string();
        let kind_value = kind_sql_value(kind).to_string();
        let agent_session_id = agent_session_id.map(|value| value.to_string());
        let text_hash = text_hash.to_string();
        runtime().block_on(async move {
            let rows = scan_all(&dataset).await?;
            let mut matches = rows
                .into_iter()
                .filter(|row| {
                    scope_sql_value(&row.scope) == scope_value
                        && kind_sql_value(&row.kind) == kind_value
                        && row.text_hash == text_hash
                        && matches!(
                            row.review_state,
                            AgentMemoryReviewState::Candidate | AgentMemoryReviewState::Approved
                        )
                        && row.agent_session_id.as_deref() == agent_session_id.as_deref()
                })
                .collect::<Vec<_>>();
            matches.sort_by(|a, b| {
                b.updated_at
                    .cmp(&a.updated_at)
                    .then_with(|| b.memory_id.cmp(&a.memory_id))
            });
            Ok(matches
                .into_iter()
                .next()
                .map(|row| stamp_project(row, &project_id))
                .map(AgentMemoryRow::into_record))
        })
    }

    pub fn get_by_memory_id(
        &self,
        memory_id: &str,
    ) -> Result<Option<AgentMemoryRecord>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let memory_id = memory_id.to_string();
        runtime().block_on(async move {
            let row = fetch_row(&dataset, &memory_id).await?;
            Ok(row
                .map(|row| stamp_project(row, &project_id))
                .map(AgentMemoryRow::into_record))
        })
    }

    pub fn update(
        &self,
        update: AgentMemoryUpdate,
        now: String,
    ) -> Result<AgentMemoryRecord, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        runtime().block_on(async move {
            let mut row = fetch_row(&dataset, &update.memory_id)
                .await?
                .ok_or_else(|| missing_memory_error(&project_id, &update.memory_id))?;
            if let Some(state) = update.review_state {
                row.review_state = state;
            }
            if let Some(enabled) = update.enabled {
                row.enabled = enabled;
            }
            if row.review_state != AgentMemoryReviewState::Approved {
                row.enabled = false;
            }
            if let Some(diagnostic) = update.diagnostic {
                row.diagnostic = Some(diagnostic);
            }
            row.updated_at = now;
            replace_row(&dataset, row.clone()).await?;
            Ok(stamp_project(row, &project_id).into_record())
        })
    }

    pub fn delete(&self, memory_id: &str) -> Result<bool, CommandError> {
        let dataset = self.dataset_dir.clone();
        let memory_id = memory_id.to_string();
        runtime().block_on(async move { delete_row(&dataset, &memory_id).await })
    }

    /// When an `agent_runs` row is removed, blank the provenance fields of any
    /// memory that referenced it so Lance records stay aligned with the
    /// relational runtime store.
    pub fn clear_runs(&self, run_ids: &[String], now: &str) -> Result<usize, CommandError> {
        if run_ids.is_empty() {
            return Ok(0);
        }
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let run_ids: Vec<String> = run_ids.iter().map(|value| value.to_string()).collect();
        let now = now.to_string();
        runtime().block_on(async move {
            let rows = scan_all(&dataset).await?;
            let mut updated = 0;
            for mut row in rows {
                if row
                    .source_run_id
                    .as_deref()
                    .map(|run_id| run_ids.iter().any(|target| target == run_id))
                    .unwrap_or(false)
                {
                    row.project_id = project_id.clone();
                    row.source_run_id = None;
                    row.source_item_ids = Vec::new();
                    row.diagnostic = Some(AgentRunDiagnosticRecord {
                        code: "memory_source_deleted".to_string(),
                        message:
                            "The source run for this memory was deleted, so Xero cleared its provenance reference."
                                .to_string(),
                    });
                    row.updated_at = now.clone();
                    replace_row(&dataset, row).await?;
                    updated += 1;
                }
            }
            Ok::<usize, CommandError>(updated)
        })
    }

    pub(crate) fn update_embedding(
        &self,
        memory_id: &str,
        embedding: Vec<f32>,
        embedding_model: String,
        embedding_dimension: i32,
        embedding_version: String,
        updated_at: String,
    ) -> Result<Option<AgentMemoryRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let memory_id = memory_id.to_string();
        runtime().block_on(async move {
            let mut row = fetch_row(&dataset, &memory_id).await?;
            let Some(mut row) = row.take() else {
                return Ok(None);
            };
            row.project_id = project_id.clone();
            row.embedding = Some(embedding);
            row.embedding_model = Some(embedding_model);
            row.embedding_dimension = Some(embedding_dimension);
            row.embedding_version = Some(embedding_version);
            row.updated_at = updated_at;
            replace_row(&dataset, row.clone()).await?;
            Ok(Some(stamp_project(row, &project_id)))
        })
    }

    pub(crate) fn update_freshness(
        &self,
        memory_id: &str,
        update: FreshnessUpdate,
    ) -> Result<Option<AgentMemoryRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let memory_id = memory_id.to_string();
        runtime().block_on(async move {
            let mut row = fetch_row(&dataset, &memory_id).await?;
            let Some(mut row) = row.take() else {
                return Ok(None);
            };
            row.project_id = project_id.clone();
            row.freshness_state = update.freshness_state.as_str().into();
            row.freshness_checked_at = update.freshness_checked_at;
            row.stale_reason = update.stale_reason;
            row.source_fingerprints_json = update.source_fingerprints_json;
            row.invalidated_at = update.invalidated_at;
            replace_row(&dataset, row.clone()).await?;
            Ok(Some(stamp_project(row, &project_id)))
        })
    }

    pub(crate) fn update_supersession(
        &self,
        memory_id: &str,
        update: SupersessionUpdate,
    ) -> Result<Option<AgentMemoryRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let memory_id = memory_id.to_string();
        runtime().block_on(async move {
            let mut row = fetch_row(&dataset, &memory_id).await?;
            let Some(mut row) = row.take() else {
                return Ok(None);
            };
            row.project_id = project_id.clone();
            if update.superseded_by_id.is_some() {
                row.freshness_state = "superseded".into();
            }
            row.superseded_by_id = update.superseded_by_id;
            row.supersedes_id = update.supersedes_id;
            row.fact_key = update.fact_key;
            row.invalidated_at = update.invalidated_at;
            row.stale_reason = update.stale_reason;
            row.updated_at = update.updated_at;
            replace_row(&dataset, row.clone()).await?;
            Ok(Some(stamp_project(row, &project_id)))
        })
    }
}

fn same_dedup_key(left: &AgentMemoryRow, right: &AgentMemoryRow) -> bool {
    if left.memory_id == right.memory_id {
        return true;
    }
    left.scope == right.scope
        && left.kind == right.kind
        && left.agent_session_id == right.agent_session_id
        && left.text_hash == right.text_hash
        && left.source_run_id == right.source_run_id
        && left.source_item_ids == right.source_item_ids
        && !matches!(left.review_state, AgentMemoryReviewState::Rejected)
}

fn ordering_for_list(rows: &mut [AgentMemoryRow]) {
    rows.sort_by(|a, b| {
        let scope_a = scope_priority(&a.scope);
        let scope_b = scope_priority(&b.scope);
        scope_a
            .cmp(&scope_b)
            .then_with(|| kind_priority(&a.kind).cmp(&kind_priority(&b.kind)))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| b.memory_id.cmp(&a.memory_id))
    });
}

fn ordering_for_list_approved(rows: &mut [AgentMemoryRow]) {
    rows.sort_by(|a, b| {
        let scope_a = scope_priority(&a.scope);
        let scope_b = scope_priority(&b.scope);
        scope_a
            .cmp(&scope_b)
            .then_with(|| kind_priority(&a.kind).cmp(&kind_priority(&b.kind)))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.memory_id.cmp(&b.memory_id))
    });
}

fn scope_priority(scope: &AgentMemoryScope) -> u8 {
    match scope {
        AgentMemoryScope::Project => 0,
        AgentMemoryScope::Session => 1,
    }
}

fn kind_priority(kind: &AgentMemoryKind) -> u8 {
    match kind {
        AgentMemoryKind::ProjectFact => 0,
        AgentMemoryKind::Decision => 1,
        AgentMemoryKind::UserPreference => 2,
        AgentMemoryKind::Troubleshooting => 3,
        AgentMemoryKind::SessionSummary => 4,
    }
}

fn filter_rows(
    rows: Vec<AgentMemoryRow>,
    agent_session_id: &Option<String>,
    filter: AgentMemoryListFilterOwned,
) -> Vec<AgentMemoryRow> {
    rows.into_iter()
        .filter(|row| {
            let scope_ok = match row.scope {
                AgentMemoryScope::Project => true,
                AgentMemoryScope::Session => match (&row.agent_session_id, agent_session_id) {
                    (Some(stored), Some(requested)) => stored == requested,
                    _ => false,
                },
            };
            if !scope_ok {
                return false;
            }
            let enabled_ok = filter.include_disabled
                || row.enabled
                || row.review_state == AgentMemoryReviewState::Candidate;
            if !enabled_ok {
                return false;
            }
            filter.include_rejected || row.review_state != AgentMemoryReviewState::Rejected
        })
        .collect()
}

fn filter_approved(
    rows: Vec<AgentMemoryRow>,
    agent_session_id: &Option<String>,
) -> Vec<AgentMemoryRow> {
    rows.into_iter()
        .filter(|row| {
            row.review_state == AgentMemoryReviewState::Approved
                && row.enabled
                && match row.scope {
                    AgentMemoryScope::Project => true,
                    AgentMemoryScope::Session => match (&row.agent_session_id, agent_session_id) {
                        (Some(stored), Some(requested)) => stored == requested,
                        _ => false,
                    },
                }
        })
        .collect()
}

fn into_records(
    rows: Vec<AgentMemoryRow>,
    project_id: &str,
    sort: fn(&mut [AgentMemoryRow]),
) -> Vec<AgentMemoryRecord> {
    let mut rows: Vec<AgentMemoryRow> = rows
        .into_iter()
        .map(|row| stamp_project(row, project_id))
        .collect();
    sort(&mut rows);
    rows.into_iter().map(AgentMemoryRow::into_record).collect()
}

fn stamp_project(mut row: AgentMemoryRow, project_id: &str) -> AgentMemoryRow {
    row.project_id = project_id.to_string();
    row
}

async fn scan_all(dataset_dir: &Path) -> Result<Vec<AgentMemoryRow>, CommandError> {
    let connection = ensure_connection(dataset_dir).await?;
    let table = match connection.open_table(AGENT_MEMORIES_TABLE).execute().await {
        Ok(table) => table,
        Err(_) => return Ok(Vec::new()),
    };
    let stream = table
        .query()
        .execute()
        .await
        .map_err(|error| map_lance_error("agent_memory_lance_query_failed", error))?;
    let batches: Vec<RecordBatch> = stream
        .try_collect()
        .await
        .map_err(|error| map_lance_error("agent_memory_lance_query_failed", error))?;
    batches_to_rows(batches)
}

async fn fetch_row(
    dataset_dir: &Path,
    memory_id: &str,
) -> Result<Option<AgentMemoryRow>, CommandError> {
    let rows = scan_all(dataset_dir).await?;
    Ok(rows.into_iter().find(|row| row.memory_id == memory_id))
}

async fn insert_row(table: &Table, row: &AgentMemoryRow) -> Result<(), CommandError> {
    let batch = build_batch(std::slice::from_ref(row))?;
    let batches: Vec<RecordBatch> = vec![batch];
    table
        .add(batches)
        .execute()
        .await
        .map_err(|error| map_lance_error("agent_memory_lance_insert_failed", error))
        .map(|_| ())
}

async fn replace_row(dataset_dir: &Path, row: AgentMemoryRow) -> Result<(), CommandError> {
    let connection = ensure_connection(dataset_dir).await?;
    let table = open_or_create_table(&connection).await?;
    let predicate = format!("memory_id = {}", quote_string_literal(&row.memory_id));
    table
        .delete(&predicate)
        .await
        .map_err(|error| map_lance_error("agent_memory_lance_update_failed", error))?;
    insert_row(&table, &row).await
}

async fn delete_row(dataset_dir: &Path, memory_id: &str) -> Result<bool, CommandError> {
    let connection = ensure_connection(dataset_dir).await?;
    let table = match connection.open_table(AGENT_MEMORIES_TABLE).execute().await {
        Ok(table) => table,
        Err(_) => return Ok(false),
    };
    let predicate = format!("memory_id = {}", quote_string_literal(memory_id));
    table
        .delete(&predicate)
        .await
        .map_err(|error| map_lance_error("agent_memory_lance_delete_failed", error))?;
    let still_present = scan_all(dataset_dir)
        .await?
        .into_iter()
        .any(|row| row.memory_id == memory_id);
    Ok(!still_present)
}

fn quote_string_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('\'');
    for ch in value.chars() {
        if ch == '\'' {
            out.push('\'');
            out.push('\'');
        } else {
            out.push(ch);
        }
    }
    out.push('\'');
    out
}

fn missing_memory_error(project_id: &str, memory_id: &str) -> CommandError {
    CommandError::user_fixable(
        "agent_memory_not_found",
        format!("Xero could not find memory `{memory_id}` for project `{project_id}`."),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::project_store::source_fingerprints_empty_json;

    fn sample_row(memory_id: &str, scope: AgentMemoryScope) -> AgentMemoryRow {
        AgentMemoryRow {
            memory_id: memory_id.into(),
            project_id: "project-test".into(),
            agent_session_id: match scope {
                AgentMemoryScope::Project => None,
                AgentMemoryScope::Session => Some("agent-session-main".into()),
            },
            scope,
            kind: AgentMemoryKind::Decision,
            text: format!("Memory body for {memory_id}"),
            text_hash: "0".repeat(64),
            review_state: AgentMemoryReviewState::Candidate,
            enabled: false,
            confidence: Some(50),
            source_run_id: Some("run-1".into()),
            source_item_ids: vec!["message:1".into()],
            diagnostic: None,
            freshness_state: "source_unknown".into(),
            freshness_checked_at: None,
            stale_reason: None,
            source_fingerprints_json: source_fingerprints_empty_json(),
            supersedes_id: None,
            superseded_by_id: None,
            invalidated_at: None,
            fact_key: None,
            created_at: "2026-04-26T00:00:00Z".into(),
            updated_at: "2026-04-26T00:00:00Z".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimension: None,
            embedding_version: None,
        }
    }

    #[test]
    fn build_batch_round_trips_a_single_row() {
        let row = sample_row("memory-a", AgentMemoryScope::Project);
        let batch = build_batch(std::slice::from_ref(&row)).expect("batch builds");
        let rows = batch_to_rows(&batch).expect("decode");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].memory_id, "memory-a");
        assert_eq!(rows[0].agent_session_id, None);
        assert_eq!(rows[0].scope, AgentMemoryScope::Project);
        assert_eq!(rows[0].kind, AgentMemoryKind::Decision);
        assert_eq!(rows[0].text, "Memory body for memory-a");
        assert_eq!(rows[0].source_item_ids, vec!["message:1".to_string()]);
        assert_eq!(rows[0].confidence, Some(50));
        assert_eq!(rows[0].freshness_state, "source_unknown");
        assert_eq!(
            rows[0].source_fingerprints_json,
            source_fingerprints_empty_json()
        );
    }

    #[test]
    fn build_batch_preserves_optional_columns_in_session_scope() {
        let mut row = sample_row("memory-b", AgentMemoryScope::Session);
        row.diagnostic = Some(AgentRunDiagnosticRecord {
            code: "memory_diag".into(),
            message: "Memory had a diagnostic.".into(),
        });
        row.confidence = None;
        row.source_run_id = None;
        row.source_item_ids = Vec::new();
        let batch = build_batch(std::slice::from_ref(&row)).expect("batch");
        let rows = batch_to_rows(&batch).expect("decode");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].agent_session_id,
            Some("agent-session-main".to_string())
        );
        assert_eq!(rows[0].confidence, None);
        assert_eq!(rows[0].source_run_id, None);
        assert!(rows[0].source_item_ids.is_empty());
        let diagnostic = rows[0].diagnostic.as_ref().expect("diagnostic kept");
        assert_eq!(diagnostic.code, "memory_diag");
    }

    #[test]
    fn quote_string_literal_doubles_inner_quotes() {
        assert_eq!(quote_string_literal("a"), "'a'");
        assert_eq!(quote_string_literal("a'b"), "'a''b'");
    }

    #[test]
    fn ordering_for_list_groups_by_scope_then_kind() {
        let mut rows = vec![
            AgentMemoryRow {
                kind: AgentMemoryKind::SessionSummary,
                scope: AgentMemoryScope::Session,
                agent_session_id: Some("s".into()),
                ..sample_row("m1", AgentMemoryScope::Session)
            },
            AgentMemoryRow {
                kind: AgentMemoryKind::ProjectFact,
                scope: AgentMemoryScope::Project,
                agent_session_id: None,
                updated_at: "2026-04-26T00:00:01Z".into(),
                ..sample_row("m2", AgentMemoryScope::Project)
            },
            AgentMemoryRow {
                kind: AgentMemoryKind::ProjectFact,
                scope: AgentMemoryScope::Project,
                agent_session_id: None,
                updated_at: "2026-04-26T00:00:02Z".into(),
                ..sample_row("m3", AgentMemoryScope::Project)
            },
        ];
        ordering_for_list(&mut rows);
        let order = rows
            .iter()
            .map(|row| row.memory_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(order, vec!["m3", "m2", "m1"]);
    }

    fn approved_project_row(memory_id: &str) -> AgentMemoryRow {
        AgentMemoryRow {
            review_state: AgentMemoryReviewState::Approved,
            enabled: true,
            ..sample_row(memory_id, AgentMemoryScope::Project)
        }
    }

    #[test]
    fn project_store_round_trip_insert_list_get_update_delete() {
        let dir = tempfile::tempdir().expect("tempdir");
        let database_path = dir.path().join("state.db");
        // Make the parent dir exist; the lance layer creates `lance/` itself.
        std::fs::create_dir_all(dir.path()).unwrap();
        reset_connection_cache_for_tests();

        let store = open_for_database_path(&database_path, "project-rt");

        let inserted = store
            .insert(approved_project_row("memory-1"))
            .expect("insert succeeds");
        assert_eq!(inserted.memory_id, "memory-1");
        assert_eq!(inserted.project_id, "project-rt");
        assert!(inserted.enabled);

        let listed = store
            .list(None, AgentMemoryListFilterOwned::default())
            .expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].memory_id, "memory-1");

        let approved = store.list_approved(None).expect("approved list");
        assert_eq!(approved.len(), 1);

        let fetched = store
            .get_by_memory_id("memory-1")
            .expect("get")
            .expect("memory exists");
        assert_eq!(fetched.text, inserted.text);

        let updated = store
            .update(
                AgentMemoryUpdate {
                    project_id: "project-rt".into(),
                    memory_id: "memory-1".into(),
                    review_state: Some(AgentMemoryReviewState::Rejected),
                    enabled: None,
                    diagnostic: None,
                },
                "2026-04-26T00:01:00Z".into(),
            )
            .expect("update");
        assert_eq!(updated.review_state, AgentMemoryReviewState::Rejected);
        // Rejection forces enabled=false even if the caller did not pass it.
        assert!(!updated.enabled);

        let removed = store.delete("memory-1").expect("delete");
        assert!(removed);
        let after = store
            .get_by_memory_id("memory-1")
            .expect("get after delete");
        assert!(after.is_none());
    }

    #[test]
    fn project_store_finds_active_by_hash_for_project_scope() {
        let dir = tempfile::tempdir().expect("tempdir");
        let database_path = dir.path().join("state.db");
        reset_connection_cache_for_tests();
        let store = open_for_database_path(&database_path, "project-hash");

        let mut row = approved_project_row("memory-h");
        row.text_hash = "a".repeat(64);
        store.insert(row).expect("insert");

        let found = store
            .find_active_by_hash(
                &AgentMemoryScope::Project,
                None,
                &AgentMemoryKind::Decision,
                &"a".repeat(64),
            )
            .expect("find");
        assert!(found.is_some());
        let found = found.unwrap();
        assert_eq!(found.memory_id, "memory-h");

        let not_found = store
            .find_active_by_hash(
                &AgentMemoryScope::Project,
                None,
                &AgentMemoryKind::Decision,
                &"b".repeat(64),
            )
            .expect("find empty");
        assert!(not_found.is_none());
    }

    #[test]
    fn project_store_clear_runs_blanks_provenance() {
        let dir = tempfile::tempdir().expect("tempdir");
        let database_path = dir.path().join("state.db");
        reset_connection_cache_for_tests();
        let store = open_for_database_path(&database_path, "project-clear");

        let mut row = approved_project_row("memory-c");
        row.source_run_id = Some("run-victim".into());
        row.source_item_ids = vec!["message:1".into()];
        store.insert(row).expect("insert");

        let updated = store
            .clear_runs(&["run-victim".to_string()], "2026-04-26T00:02:00Z")
            .expect("clear");
        assert_eq!(updated, 1);

        let fetched = store
            .get_by_memory_id("memory-c")
            .expect("get")
            .expect("still exists");
        assert!(fetched.source_run_id.is_none());
        assert!(fetched.source_item_ids.is_empty());
        let diagnostic = fetched.diagnostic.expect("diagnostic populated");
        assert_eq!(diagnostic.code, "memory_source_deleted");
        assert_eq!(fetched.updated_at, "2026-04-26T00:02:00Z");
    }

    #[test]
    fn project_store_session_scope_filter_excludes_other_sessions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let database_path = dir.path().join("state.db");
        reset_connection_cache_for_tests();
        let store = open_for_database_path(&database_path, "project-scope");

        let mut session_a = sample_row("memory-sa", AgentMemoryScope::Session);
        session_a.agent_session_id = Some("session-a".into());
        session_a.review_state = AgentMemoryReviewState::Approved;
        session_a.enabled = true;
        store.insert(session_a).expect("insert session a");

        let mut session_b = sample_row("memory-sb", AgentMemoryScope::Session);
        session_b.agent_session_id = Some("session-b".into());
        session_b.review_state = AgentMemoryReviewState::Approved;
        session_b.enabled = true;
        store.insert(session_b).expect("insert session b");

        let project_only = approved_project_row("memory-p");
        store.insert(project_only).expect("insert project");

        let visible_to_a = store
            .list_approved(Some("session-a"))
            .expect("approved for a");
        let ids: Vec<_> = visible_to_a
            .iter()
            .map(|record| record.memory_id.clone())
            .collect();
        assert!(ids.contains(&"memory-sa".to_string()));
        assert!(ids.contains(&"memory-p".to_string()));
        assert!(!ids.contains(&"memory-sb".to_string()));
    }

    #[test]
    fn filter_rows_respects_session_scope_match() {
        let mut session_row = sample_row("session-mem", AgentMemoryScope::Session);
        session_row.agent_session_id = Some("session-a".into());
        session_row.review_state = AgentMemoryReviewState::Approved;
        session_row.enabled = true;

        let mut project_row = sample_row("project-mem", AgentMemoryScope::Project);
        project_row.review_state = AgentMemoryReviewState::Approved;
        project_row.enabled = true;

        let rows = vec![session_row.clone(), project_row.clone()];
        let only_a = filter_rows(
            rows.clone(),
            &Some("session-a".into()),
            AgentMemoryListFilterOwned::default(),
        );
        let ids: Vec<_> = only_a.iter().map(|row| row.memory_id.clone()).collect();
        assert!(ids.contains(&"session-mem".to_string()));
        assert!(ids.contains(&"project-mem".to_string()));

        let other_session = filter_rows(
            rows,
            &Some("session-b".into()),
            AgentMemoryListFilterOwned::default(),
        );
        let ids: Vec<_> = other_session
            .iter()
            .map(|row| row.memory_id.clone())
            .collect();
        assert!(ids.contains(&"project-mem".to_string()));
        assert!(!ids.contains(&"session-mem".to_string()));
    }
}
