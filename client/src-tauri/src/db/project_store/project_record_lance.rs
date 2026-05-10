//! Lance-backed project records.
//!
//! Stores typed workflow/retrieval records under the same per-project
//! app-data Lance directory used by reviewed agent memories. These records are
//! runtime-owned knowledge artifacts, not model-callable mutation tools.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex, OnceLock};

use arrow_array::builder::{
    FixedSizeListBuilder, Float32Builder, Float64Builder, Int32Builder, StringBuilder,
};
use arrow_array::{
    Array, ArrayRef, FixedSizeListArray, Float64Array, Int32Array, RecordBatch,
    RecordBatchIterator, StringArray,
};
use arrow_schema::{DataType, Field, Schema, SchemaRef};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use lancedb::table::OptimizeAction;
use lancedb::{Connection, DistanceType, Table};
use serde::{Deserialize, Serialize};
use tokio::runtime::Runtime;

use crate::commands::{CommandError, RuntimeAgentIdDto};

use super::{
    agent_embeddings::AGENT_RETRIEVAL_EMBEDDING_DIM, agent_memory_lance::PROJECT_LANCE_SUBDIR,
    lance_health, FreshnessUpdate, SupersessionUpdate,
};

pub const PROJECT_RECORD_EMBEDDING_DIM: i32 = AGENT_RETRIEVAL_EMBEDDING_DIM;
const PROJECT_RECORDS_TABLE: &str = "project_records";
const PROJECT_RECORD_EMBEDDING_COLUMN: &str = "embedding";
const PROJECT_RECORD_EMBEDDING_INDEX: &str = "project_records_embedding_cosine_idx";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectRecordRow {
    pub record_id: String,
    pub project_id: String,
    pub record_kind: String,
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
    pub content_json: Option<String>,
    pub content_hash: Option<String>,
    pub schema_name: Option<String>,
    pub schema_version: i32,
    pub importance: String,
    pub confidence: Option<f64>,
    pub tags_json: String,
    pub source_item_ids_json: String,
    pub related_paths_json: String,
    pub produced_artifact_refs_json: String,
    pub redaction_state: String,
    pub visibility: String,
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

pub fn dataset_dir_for_database_path(database_path: &Path) -> PathBuf {
    database_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join(PROJECT_LANCE_SUBDIR)
}

pub fn schema() -> SchemaRef {
    Arc::new(Schema::new(vec![
        Field::new("record_id", DataType::Utf8, false),
        Field::new("record_kind", DataType::Utf8, false),
        Field::new("runtime_agent_id", DataType::Utf8, false),
        Field::new("agent_definition_id", DataType::Utf8, false),
        Field::new("agent_definition_version", DataType::Int32, false),
        Field::new("agent_session_id", DataType::Utf8, true),
        Field::new("run_id", DataType::Utf8, false),
        Field::new("workflow_run_id", DataType::Utf8, true),
        Field::new("workflow_step_id", DataType::Utf8, true),
        Field::new("title", DataType::Utf8, false),
        Field::new("summary", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new("text_hash", DataType::Utf8, false),
        Field::new("content_json", DataType::Utf8, true),
        Field::new("content_hash", DataType::Utf8, true),
        Field::new("schema_name", DataType::Utf8, true),
        Field::new("schema_version", DataType::Int32, false),
        Field::new("importance", DataType::Utf8, false),
        Field::new("confidence", DataType::Float64, true),
        Field::new("tags_json", DataType::Utf8, false),
        Field::new("source_item_ids_json", DataType::Utf8, false),
        Field::new("related_paths_json", DataType::Utf8, false),
        Field::new("produced_artifact_refs_json", DataType::Utf8, false),
        Field::new("redaction_state", DataType::Utf8, false),
        Field::new("visibility", DataType::Utf8, false),
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
                PROJECT_RECORD_EMBEDDING_DIM,
            ),
            true,
        ),
    ]))
}

pub(crate) fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("xero-project-record-lance")
            .enable_all()
            .build()
            .expect("xero project record lance tokio runtime")
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

pub(crate) fn clear_connection_cache_for_database_path(database_path: &Path) {
    let dataset_dir = dataset_dir_for_database_path(database_path);
    if let Ok(mut cache) = connection_cache().lock() {
        cache.inner.remove(&dataset_dir);
    }
}

#[cfg(test)]
pub fn reset_connection_cache_for_tests() {
    if let Ok(mut cache) = connection_cache().lock() {
        cache.inner.clear();
    }
}

fn map_lance_error<E: std::fmt::Display>(code: &'static str, error: E) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero project_records lance store failed: {error}"),
    )
}

async fn connect_dataset(dataset_dir: &Path) -> Result<Connection, CommandError> {
    std::fs::create_dir_all(dataset_dir).map_err(|error| {
        CommandError::retryable(
            "project_record_lance_dir_unavailable",
            format!(
                "Xero could not prepare the project-record lance dataset directory at {}: {error}",
                dataset_dir.display()
            ),
        )
    })?;
    let uri = dataset_dir
        .to_str()
        .ok_or_else(|| {
            CommandError::system_fault(
                "project_record_lance_dir_non_utf8",
                format!(
                    "Xero cannot use non-UTF8 project-record lance path {}.",
                    dataset_dir.display()
                ),
            )
        })?
        .to_string();
    lancedb::connect(&uri)
        .execute()
        .await
        .map_err(|error| map_lance_error("project_record_lance_connect_failed", error))
}

async fn ensure_connection(dataset_dir: &Path) -> Result<Connection, CommandError> {
    {
        let cache = connection_cache()
            .lock()
            .expect("project record lance cache poisoned");
        if let Some(connection) = cache.inner.get(dataset_dir).cloned() {
            return Ok(connection);
        }
    }
    let connection = connect_dataset(dataset_dir).await?;
    let mut cache = connection_cache()
        .lock()
        .expect("project record lance cache poisoned");
    cache
        .inner
        .insert(dataset_dir.to_path_buf(), connection.clone());
    Ok(connection)
}

async fn open_or_create_table(
    connection: &Connection,
    dataset_dir: &Path,
) -> Result<Table, CommandError> {
    match connection.open_table(PROJECT_RECORDS_TABLE).execute().await {
        Ok(table) => ensure_current_table_schema(connection, dataset_dir, table).await,
        Err(_) => create_project_records_table(connection).await,
    }
}

async fn ensure_current_table_schema(
    connection: &Connection,
    dataset_dir: &Path,
    table: Table,
) -> Result<Table, CommandError> {
    let table_schema = match table.schema().await {
        Ok(table_schema) => table_schema,
        Err(_) => {
            drop(table);
            return quarantine_and_recreate_project_records_table(
                connection,
                dataset_dir,
                "project_record_lance_schema_read_failed",
            )
            .await;
        }
    };
    if table_schema_supports_current_records(&table_schema) {
        return Ok(table);
    }
    drop(table);
    quarantine_and_recreate_project_records_table(
        connection,
        dataset_dir,
        "project_record_lance_schema_drift_quarantined",
    )
    .await
}

fn table_schema_supports_current_records(table_schema: &Schema) -> bool {
    lance_health::table_schema_supports_expected(table_schema, schema().as_ref())
}

async fn quarantine_and_recreate_project_records_table(
    connection: &Connection,
    dataset_dir: &Path,
    reason_code: &'static str,
) -> Result<Table, CommandError> {
    let _quarantined = lance_health::quarantine_lance_table_directory(
        dataset_dir,
        PROJECT_RECORDS_TABLE,
        "project-record",
        reason_code,
    )?;
    create_project_records_table(connection).await
}

async fn create_project_records_table(connection: &Connection) -> Result<Table, CommandError> {
    let schema = schema();
    let empty = RecordBatch::new_empty(schema.clone());
    let iter = RecordBatchIterator::new(
        std::iter::once(Ok::<_, arrow_schema::ArrowError>(empty)),
        schema,
    );
    let reader: Box<dyn arrow_array::RecordBatchReader + Send + 'static> = Box::new(iter);
    match connection
        .create_table(PROJECT_RECORDS_TABLE, reader)
        .execute()
        .await
    {
        Ok(table) => Ok(table),
        Err(create_error) => match connection.open_table(PROJECT_RECORDS_TABLE).execute().await {
            Ok(table) => match table.schema().await {
                Ok(table_schema) if table_schema_supports_current_records(&table_schema) => {
                    Ok(table)
                }
                Ok(_) => Err(CommandError::retryable(
                    "project_record_lance_create_table_failed",
                    format!(
                        "Xero project_records lance store failed: {create_error}; retry open found a stale schema"
                    ),
                )),
                Err(schema_error) => Err(CommandError::retryable(
                    "project_record_lance_create_table_failed",
                    format!(
                        "Xero project_records lance store failed: {create_error}; retry open schema failed: {schema_error}"
                    ),
                )),
            },
            Err(open_error) => Err(CommandError::retryable(
                "project_record_lance_create_table_failed",
                format!(
                    "Xero project_records lance store failed: {create_error}; retry open failed: {open_error}"
                ),
            )),
        },
    }
}

pub struct ProjectRecordStore {
    project_id: String,
    dataset_dir: PathBuf,
}

pub fn open_for_database_path(database_path: &Path, project_id: &str) -> ProjectRecordStore {
    ProjectRecordStore {
        project_id: project_id.to_string(),
        dataset_dir: dataset_dir_for_database_path(database_path),
    }
}

impl ProjectRecordStore {
    #[allow(dead_code)]
    pub(crate) fn health_report(
        &self,
    ) -> Result<lance_health::LanceTableHealthReport, CommandError> {
        let dataset = self.dataset_dir.clone();
        runtime().block_on(async move { health_report_for_dataset(&dataset).await })
    }

    #[allow(dead_code)]
    pub(crate) fn optimize_for_maintenance(
        &self,
    ) -> Result<lance_health::LanceTableOptimizationReport, CommandError> {
        let dataset = self.dataset_dir.clone();
        runtime().block_on(async move {
            let connection = ensure_connection(&dataset).await?;
            let table = open_or_create_table(&connection, &dataset).await?;
            let _ = maintain_embedding_index(&table, false).await?;
            let before = health_report_for_dataset(&dataset).await?;
            let stats = table
                .optimize(OptimizeAction::All)
                .await
                .map_err(|error| map_lance_error("project_record_lance_optimize_failed", error))?;
            let after = health_report_for_dataset(&dataset).await?;
            Ok(lance_health::table_optimization_report(
                PROJECT_RECORDS_TABLE,
                before,
                &stats,
                after,
            ))
        })
    }

    pub fn insert_dedup(
        &self,
        mut row: ProjectRecordRow,
    ) -> Result<ProjectRecordRow, CommandError> {
        row.project_id = self.project_id.clone();
        row.updated_at = row.created_at.clone();
        let dataset = self.dataset_dir.clone();
        runtime().block_on(async move {
            let mut rows = scan_all(&dataset).await?;
            if let Some(existing) = rows.iter().find(|existing| same_dedup_key(existing, &row)) {
                return Ok(existing.clone());
            }
            let connection = ensure_connection(&dataset).await?;
            let table = open_or_create_table(&connection, &dataset).await?;
            insert_row(&table, &row).await?;
            rows.push(row.clone());
            Ok(row)
        })
    }

    pub fn list(&self) -> Result<Vec<ProjectRecordRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        runtime().block_on(async move {
            let mut rows = scan_all(&dataset).await?;
            for row in &mut rows {
                row.project_id = project_id.clone();
            }
            rows.sort_by(|left, right| {
                right
                    .created_at
                    .cmp(&left.created_at)
                    .then_with(|| right.record_id.cmp(&left.record_id))
            });
            Ok(rows)
        })
    }

    pub(crate) fn list_rows(&self) -> Result<Vec<ProjectRecordRow>, CommandError> {
        self.list()
    }

    pub fn delete(&self, record_id: &str) -> Result<bool, CommandError> {
        let dataset = self.dataset_dir.clone();
        let record_id = record_id.to_string();
        runtime().block_on(async move { delete_row(&dataset, &record_id).await })
    }

    pub(crate) fn vector_search_rows(
        &self,
        query_embedding: &[f32],
        limit: usize,
        filter_sql: Option<&str>,
    ) -> Result<Vec<ProjectRecordRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let query_embedding = query_embedding.to_vec();
        let filter_sql = filter_sql.map(str::to_owned);
        runtime().block_on(async move {
            let connection = ensure_connection(&dataset).await?;
            let table = open_or_create_table(&connection, &dataset).await?;
            let _ = maintain_embedding_index(&table, false).await?;
            let mut query = table
                .query()
                .nearest_to(query_embedding)
                .map_err(|error| {
                    map_lance_error("project_record_lance_vector_query_invalid", error)
                })?
                .distance_type(DistanceType::Cosine)
                .limit(limit.max(1));
            if let Some(filter_sql) = filter_sql.as_deref().filter(|filter| !filter.is_empty()) {
                query = query.only_if(filter_sql);
            }
            let batches = query
                .execute()
                .await
                .map_err(|error| {
                    map_lance_error("project_record_lance_vector_query_failed", error)
                })?
                .try_collect::<Vec<_>>()
                .await
                .map_err(|error| {
                    map_lance_error("project_record_lance_vector_collect_failed", error)
                })?;
            let mut rows = batches_to_rows(batches)?;
            for row in &mut rows {
                row.project_id = project_id.clone();
            }
            Ok(rows)
        })
    }

    pub(crate) fn update_embedding(
        &self,
        record_id: &str,
        embedding: Vec<f32>,
        embedding_model: String,
        embedding_dimension: i32,
        embedding_version: String,
        updated_at: String,
    ) -> Result<Option<ProjectRecordRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let record_id = record_id.to_string();
        runtime().block_on(async move {
            let mut row = fetch_row(&dataset, &record_id).await?;
            let Some(previous) = row.take() else {
                return Ok(None);
            };
            let mut updated = previous.clone();
            updated.project_id = project_id.clone();
            updated.embedding = Some(embedding);
            updated.embedding_model = Some(embedding_model);
            updated.embedding_dimension = Some(embedding_dimension);
            updated.embedding_version = Some(embedding_version);
            updated.updated_at = updated_at;
            replace_row(&dataset, &previous, updated.clone()).await?;
            Ok(Some(updated))
        })
    }

    pub(crate) fn update_freshness(
        &self,
        record_id: &str,
        update: FreshnessUpdate,
    ) -> Result<Option<ProjectRecordRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let record_id = record_id.to_string();
        runtime().block_on(async move {
            let mut row = fetch_row(&dataset, &record_id).await?;
            let Some(previous) = row.take() else {
                return Ok(None);
            };
            let mut updated = previous.clone();
            updated.project_id = project_id;
            updated.updated_at = next_row_version_timestamp(
                &previous.updated_at,
                update
                    .freshness_checked_at
                    .clone()
                    .or_else(|| update.invalidated_at.clone())
                    .unwrap_or_else(|| previous.updated_at.clone()),
            );
            updated.freshness_state = update.freshness_state.as_str().into();
            updated.freshness_checked_at = update.freshness_checked_at;
            updated.stale_reason = update.stale_reason;
            updated.source_fingerprints_json = update.source_fingerprints_json;
            updated.invalidated_at = update.invalidated_at;
            replace_row(&dataset, &previous, updated.clone()).await?;
            Ok(Some(updated))
        })
    }

    pub(crate) fn update_supersession(
        &self,
        record_id: &str,
        update: SupersessionUpdate,
    ) -> Result<Option<ProjectRecordRow>, CommandError> {
        let dataset = self.dataset_dir.clone();
        let project_id = self.project_id.clone();
        let record_id = record_id.to_string();
        runtime().block_on(async move {
            let mut row = fetch_row(&dataset, &record_id).await?;
            let Some(previous) = row.take() else {
                return Ok(None);
            };
            let mut updated = previous.clone();
            updated.project_id = project_id;
            if update.superseded_by_id.is_some() {
                updated.freshness_state = "superseded".into();
            }
            updated.superseded_by_id = update.superseded_by_id;
            updated.supersedes_id = update.supersedes_id;
            updated.fact_key = update.fact_key;
            updated.invalidated_at = update.invalidated_at;
            updated.stale_reason = update.stale_reason;
            updated.updated_at =
                next_row_version_timestamp(&previous.updated_at, update.updated_at);
            replace_row(&dataset, &previous, updated.clone()).await?;
            Ok(Some(updated))
        })
    }
}

async fn health_report_for_dataset(
    dataset: &Path,
) -> Result<lance_health::LanceTableHealthReport, CommandError> {
    let connection = ensure_connection(dataset).await?;
    let table = open_or_create_table(&connection, dataset).await?;
    let table_schema = table
        .schema()
        .await
        .map_err(|error| map_lance_error("project_record_lance_health_schema_failed", error))?;
    let schema_current = table_schema_supports_current_records(&table_schema);
    let mut report =
        lance_health::table_health_report(&table, dataset, PROJECT_RECORDS_TABLE, schema_current)
            .await?;
    if schema_current {
        let rows = scan_all(dataset).await?;
        report.freshness_counts = lance_health::freshness_counts_from_states(
            rows.iter().map(|row| row.freshness_state.as_str()),
        );
    }
    Ok(report)
}

async fn maintain_embedding_index(
    table: &Table,
    optimize: bool,
) -> Result<lance_health::LanceVectorIndexMaintenanceReport, CommandError> {
    lance_health::maintain_cosine_vector_index(
        table,
        PROJECT_RECORDS_TABLE,
        PROJECT_RECORD_EMBEDDING_COLUMN,
        PROJECT_RECORD_EMBEDDING_INDEX,
        optimize,
    )
    .await
}

fn same_dedup_key(left: &ProjectRecordRow, right: &ProjectRecordRow) -> bool {
    left.run_id == right.run_id
        && left.record_kind == right.record_kind
        && left.text_hash == right.text_hash
        && left.content_hash == right.content_hash
        && left.related_paths_json == right.related_paths_json
}

fn build_batch(rows: &[ProjectRecordRow]) -> Result<RecordBatch, CommandError> {
    let schema = schema();
    let mut record_id = StringBuilder::new();
    let mut record_kind = StringBuilder::new();
    let mut runtime_agent_id = StringBuilder::new();
    let mut agent_definition_id = StringBuilder::new();
    let mut agent_definition_version = Int32Builder::new();
    let mut agent_session_id = StringBuilder::new();
    let mut run_id = StringBuilder::new();
    let mut workflow_run_id = StringBuilder::new();
    let mut workflow_step_id = StringBuilder::new();
    let mut title = StringBuilder::new();
    let mut summary = StringBuilder::new();
    let mut text = StringBuilder::new();
    let mut text_hash = StringBuilder::new();
    let mut content_json = StringBuilder::new();
    let mut content_hash = StringBuilder::new();
    let mut schema_name = StringBuilder::new();
    let mut schema_version = Int32Builder::new();
    let mut importance = StringBuilder::new();
    let mut confidence = Float64Builder::new();
    let mut tags_json = StringBuilder::new();
    let mut source_item_ids_json = StringBuilder::new();
    let mut related_paths_json = StringBuilder::new();
    let mut produced_artifact_refs_json = StringBuilder::new();
    let mut redaction_state = StringBuilder::new();
    let mut visibility = StringBuilder::new();
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
        FixedSizeListBuilder::new(Float32Builder::new(), PROJECT_RECORD_EMBEDDING_DIM);

    for row in rows {
        record_id.append_value(&row.record_id);
        record_kind.append_value(&row.record_kind);
        runtime_agent_id.append_value(row.runtime_agent_id.as_str());
        agent_definition_id.append_value(&row.agent_definition_id);
        agent_definition_version.append_value(row.agent_definition_version as i32);
        append_optional(&mut agent_session_id, row.agent_session_id.as_deref());
        run_id.append_value(&row.run_id);
        append_optional(&mut workflow_run_id, row.workflow_run_id.as_deref());
        append_optional(&mut workflow_step_id, row.workflow_step_id.as_deref());
        title.append_value(&row.title);
        summary.append_value(&row.summary);
        text.append_value(&row.text);
        text_hash.append_value(&row.text_hash);
        append_optional(&mut content_json, row.content_json.as_deref());
        append_optional(&mut content_hash, row.content_hash.as_deref());
        append_optional(&mut schema_name, row.schema_name.as_deref());
        schema_version.append_value(row.schema_version);
        importance.append_value(&row.importance);
        match row.confidence {
            Some(value) => confidence.append_value(value),
            None => confidence.append_null(),
        }
        tags_json.append_value(&row.tags_json);
        source_item_ids_json.append_value(&row.source_item_ids_json);
        related_paths_json.append_value(&row.related_paths_json);
        produced_artifact_refs_json.append_value(&row.produced_artifact_refs_json);
        redaction_state.append_value(&row.redaction_state);
        visibility.append_value(&row.visibility);
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
        append_optional(&mut embedding_model, row.embedding_model.as_deref());
        match row.embedding_dimension {
            Some(value) => embedding_dimension.append_value(value),
            None => embedding_dimension.append_null(),
        }
        append_optional(&mut embedding_version, row.embedding_version.as_deref());
        append_embedding(&mut embedding, row.embedding.as_deref())?;
    }

    let columns: Vec<ArrayRef> = vec![
        Arc::new(record_id.finish()),
        Arc::new(record_kind.finish()),
        Arc::new(runtime_agent_id.finish()),
        Arc::new(agent_definition_id.finish()),
        Arc::new(agent_definition_version.finish()),
        Arc::new(agent_session_id.finish()),
        Arc::new(run_id.finish()),
        Arc::new(workflow_run_id.finish()),
        Arc::new(workflow_step_id.finish()),
        Arc::new(title.finish()),
        Arc::new(summary.finish()),
        Arc::new(text.finish()),
        Arc::new(text_hash.finish()),
        Arc::new(content_json.finish()),
        Arc::new(content_hash.finish()),
        Arc::new(schema_name.finish()),
        Arc::new(schema_version.finish()),
        Arc::new(importance.finish()),
        Arc::new(confidence.finish()),
        Arc::new(tags_json.finish()),
        Arc::new(source_item_ids_json.finish()),
        Arc::new(related_paths_json.finish()),
        Arc::new(produced_artifact_refs_json.finish()),
        Arc::new(redaction_state.finish()),
        Arc::new(visibility.finish()),
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
            "project_record_lance_record_batch_failed",
            format!("Xero could not assemble project-record lance batch: {error}"),
        )
    })
}

fn append_optional(builder: &mut StringBuilder, value: Option<&str>) {
    match value {
        Some(value) => builder.append_value(value),
        None => builder.append_null(),
    }
}

fn append_embedding(
    builder: &mut FixedSizeListBuilder<Float32Builder>,
    embedding: Option<&[f32]>,
) -> Result<(), CommandError> {
    match embedding {
        Some(values) if values.len() == PROJECT_RECORD_EMBEDDING_DIM as usize => {
            for value in values {
                builder.values().append_value(*value);
            }
            builder.append(true);
            Ok(())
        }
        Some(values) => Err(CommandError::system_fault(
            "project_record_lance_embedding_dimension_mismatch",
            format!(
                "Xero project-record embedding has {} dimensions; expected {}.",
                values.len(),
                PROJECT_RECORD_EMBEDDING_DIM
            ),
        )),
        None => {
            for _ in 0..PROJECT_RECORD_EMBEDDING_DIM {
                builder.values().append_null();
            }
            builder.append(false);
            Ok(())
        }
    }
}

async fn scan_all(dataset_dir: &Path) -> Result<Vec<ProjectRecordRow>, CommandError> {
    let connection = ensure_connection(dataset_dir).await?;
    let table = open_or_create_table(&connection, dataset_dir).await?;
    let stream = table
        .query()
        .execute()
        .await
        .map_err(|error| map_lance_error("project_record_lance_query_failed", error))?;
    let batches: Vec<RecordBatch> = stream
        .try_collect()
        .await
        .map_err(|error| map_lance_error("project_record_lance_query_failed", error))?;
    Ok(dedupe_latest_record_rows(batches_to_rows(batches)?))
}

async fn insert_row(table: &Table, row: &ProjectRecordRow) -> Result<(), CommandError> {
    let batch = build_batch(std::slice::from_ref(row))?;
    table
        .add(vec![batch])
        .execute()
        .await
        .map_err(|error| map_lance_error("project_record_lance_insert_failed", error))
        .map(|_| ())
}

async fn fetch_row(
    dataset_dir: &Path,
    record_id: &str,
) -> Result<Option<ProjectRecordRow>, CommandError> {
    let rows = scan_all(dataset_dir).await?;
    Ok(rows.into_iter().find(|row| row.record_id == record_id))
}

async fn replace_row(
    dataset_dir: &Path,
    previous: &ProjectRecordRow,
    row: ProjectRecordRow,
) -> Result<(), CommandError> {
    if previous == &row {
        return Ok(());
    }
    if previous.updated_at == row.updated_at {
        return Err(CommandError::system_fault(
            "project_record_lance_replace_version_not_advanced",
            format!(
                "Xero refused to replace project record `{}` without an updated row version.",
                row.record_id
            ),
        ));
    }
    let connection = ensure_connection(dataset_dir).await?;
    let table = open_or_create_table(&connection, dataset_dir).await?;
    insert_row(&table, &row).await?;
    let predicate = format!(
        "record_id = {} AND updated_at = {}",
        quote_string_literal(&previous.record_id),
        quote_string_literal(&previous.updated_at)
    );
    table
        .delete(&predicate)
        .await
        .map_err(|error| map_lance_error("project_record_lance_update_cleanup_failed", error))?;
    Ok(())
}

fn next_row_version_timestamp(previous_updated_at: &str, candidate_updated_at: String) -> String {
    if previous_updated_at != candidate_updated_at {
        return candidate_updated_at;
    }
    advance_rfc3339_timestamp_nanos(previous_updated_at)
}

fn advance_rfc3339_timestamp_nanos(timestamp: &str) -> String {
    let Some(body) = timestamp.strip_suffix('Z') else {
        return format!("{timestamp}.000000001Z");
    };
    let (base, fraction) = match body.rsplit_once('.') {
        Some((base, fraction)) if fraction.chars().all(|character| character.is_ascii_digit()) => {
            (base, fraction)
        }
        _ => (body, ""),
    };
    let mut normalized = fraction.chars().take(9).collect::<String>();
    while normalized.len() < 9 {
        normalized.push('0');
    }
    let nanos = normalized.parse::<u32>().unwrap_or(0).saturating_add(1);
    if nanos <= 999_999_999 {
        return format!("{base}.{nanos:09}Z");
    }
    format!("{body}1Z")
}

async fn delete_row(dataset_dir: &Path, record_id: &str) -> Result<bool, CommandError> {
    let connection = ensure_connection(dataset_dir).await?;
    let table = open_or_create_table(&connection, dataset_dir).await?;
    let was_present = scan_all(dataset_dir)
        .await?
        .into_iter()
        .any(|row| row.record_id == record_id);
    if !was_present {
        return Ok(false);
    }
    let predicate = format!("record_id = {}", quote_string_literal(record_id));
    table
        .delete(&predicate)
        .await
        .map_err(|error| map_lance_error("project_record_lance_delete_failed", error))?;
    let still_present = scan_all(dataset_dir)
        .await?
        .into_iter()
        .any(|row| row.record_id == record_id);
    Ok(!still_present)
}

fn dedupe_latest_record_rows(mut rows: Vec<ProjectRecordRow>) -> Vec<ProjectRecordRow> {
    rows.sort_by(|left, right| {
        left.record_id
            .cmp(&right.record_id)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
    rows.dedup_by(|left, right| left.record_id == right.record_id);
    rows
}

pub(crate) fn quote_string_literal(value: &str) -> String {
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

fn batches_to_rows(batches: Vec<RecordBatch>) -> Result<Vec<ProjectRecordRow>, CommandError> {
    let mut rows = Vec::new();
    for batch in batches {
        rows.extend(batch_to_rows(&batch)?);
    }
    Ok(rows)
}

fn batch_to_rows(batch: &RecordBatch) -> Result<Vec<ProjectRecordRow>, CommandError> {
    let row_count = batch.num_rows();
    if row_count == 0 {
        return Ok(Vec::new());
    }

    let record_id = column_str(batch, "record_id")?;
    let record_kind = column_str(batch, "record_kind")?;
    let runtime_agent_id = column_str(batch, "runtime_agent_id")?;
    let agent_definition_id = column_str(batch, "agent_definition_id")?;
    let agent_definition_version = column_i32(batch, "agent_definition_version")?;
    let agent_session_id = column_str(batch, "agent_session_id")?;
    let run_id = column_str(batch, "run_id")?;
    let workflow_run_id = column_str(batch, "workflow_run_id")?;
    let workflow_step_id = column_str(batch, "workflow_step_id")?;
    let title = column_str(batch, "title")?;
    let summary = column_str(batch, "summary")?;
    let text = column_str(batch, "text")?;
    let text_hash = column_str(batch, "text_hash")?;
    let content_json = column_str(batch, "content_json")?;
    let content_hash = column_str(batch, "content_hash")?;
    let schema_name = column_str(batch, "schema_name")?;
    let schema_version = column_i32(batch, "schema_version")?;
    let importance = column_str(batch, "importance")?;
    let confidence = column_f64(batch, "confidence")?;
    let tags_json = column_str(batch, "tags_json")?;
    let source_item_ids_json = column_str(batch, "source_item_ids_json")?;
    let related_paths_json = column_str(batch, "related_paths_json")?;
    let produced_artifact_refs_json = column_str(batch, "produced_artifact_refs_json")?;
    let redaction_state = column_str(batch, "redaction_state")?;
    let visibility = column_str(batch, "visibility")?;
    let freshness_state = column_str(batch, "freshness_state")?;
    let freshness_checked_at = column_str(batch, "freshness_checked_at")?;
    let stale_reason = column_str(batch, "stale_reason")?;
    let source_fingerprints_json = column_str(batch, "source_fingerprints_json")?;
    let supersedes_id = column_str(batch, "supersedes_id")?;
    let superseded_by_id = column_str(batch, "superseded_by_id")?;
    let invalidated_at = column_str(batch, "invalidated_at")?;
    let fact_key = column_str(batch, "fact_key")?;
    let created_at = column_str(batch, "created_at")?;
    let updated_at = column_str(batch, "updated_at")?;
    let embedding_model = column_str(batch, "embedding_model")?;
    let embedding_dimension = column_i32(batch, "embedding_dimension")?;
    let embedding_version = column_str(batch, "embedding_version")?;
    let embedding = column_embedding(batch, "embedding")?;

    let mut rows = Vec::with_capacity(row_count);
    for index in 0..row_count {
        rows.push(ProjectRecordRow {
            record_id: require_str(record_id, index, "record_id")?.to_string(),
            project_id: String::new(),
            record_kind: require_str(record_kind, index, "record_kind")?.to_string(),
            runtime_agent_id: parse_runtime_agent_id(require_str(
                runtime_agent_id,
                index,
                "runtime_agent_id",
            )?),
            agent_definition_id: require_str(agent_definition_id, index, "agent_definition_id")?
                .to_string(),
            agent_definition_version: read_required_u32(
                agent_definition_version,
                index,
                "agent_definition_version",
            )?,
            agent_session_id: optional_str(agent_session_id, index),
            run_id: require_str(run_id, index, "run_id")?.to_string(),
            workflow_run_id: optional_str(workflow_run_id, index),
            workflow_step_id: optional_str(workflow_step_id, index),
            title: require_str(title, index, "title")?.to_string(),
            summary: require_str(summary, index, "summary")?.to_string(),
            text: require_str(text, index, "text")?.to_string(),
            text_hash: require_str(text_hash, index, "text_hash")?.to_string(),
            content_json: optional_str(content_json, index),
            content_hash: optional_str(content_hash, index),
            schema_name: optional_str(schema_name, index),
            schema_version: schema_version.value(index),
            importance: require_str(importance, index, "importance")?.to_string(),
            confidence: if confidence.is_null(index) {
                None
            } else {
                Some(confidence.value(index))
            },
            tags_json: require_str(tags_json, index, "tags_json")?.to_string(),
            source_item_ids_json: require_str(source_item_ids_json, index, "source_item_ids_json")?
                .to_string(),
            related_paths_json: require_str(related_paths_json, index, "related_paths_json")?
                .to_string(),
            produced_artifact_refs_json: require_str(
                produced_artifact_refs_json,
                index,
                "produced_artifact_refs_json",
            )?
            .to_string(),
            redaction_state: require_str(redaction_state, index, "redaction_state")?.to_string(),
            visibility: require_str(visibility, index, "visibility")?.to_string(),
            freshness_state: require_str(freshness_state, index, "freshness_state")?.to_string(),
            freshness_checked_at: optional_str(freshness_checked_at, index),
            stale_reason: optional_str(stale_reason, index),
            source_fingerprints_json: require_str(
                source_fingerprints_json,
                index,
                "source_fingerprints_json",
            )?
            .to_string(),
            supersedes_id: optional_str(supersedes_id, index),
            superseded_by_id: optional_str(superseded_by_id, index),
            invalidated_at: optional_str(invalidated_at, index),
            fact_key: optional_str(fact_key, index),
            created_at: require_str(created_at, index, "created_at")?.to_string(),
            updated_at: require_str(updated_at, index, "updated_at")?.to_string(),
            embedding: optional_embedding(embedding, index)?,
            embedding_model: optional_str(embedding_model, index),
            embedding_dimension: if embedding_dimension.is_null(index) {
                None
            } else {
                Some(embedding_dimension.value(index))
            },
            embedding_version: optional_str(embedding_version, index),
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

fn column_i32<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Int32Array, CommandError> {
    batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<Int32Array>())
        .ok_or_else(|| missing_column(name))
}

fn column_f64<'a>(batch: &'a RecordBatch, name: &str) -> Result<&'a Float64Array, CommandError> {
    batch
        .column_by_name(name)
        .and_then(|array| array.as_any().downcast_ref::<Float64Array>())
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
    lance_health::schema_drift_error("project_record_lance_schema_drift", "project-record", name)
}

fn require_str<'a>(
    array: &'a StringArray,
    index: usize,
    column: &str,
) -> Result<&'a str, CommandError> {
    if array.is_null(index) {
        return Err(CommandError::system_fault(
            "project_record_lance_unexpected_null",
            format!("Xero project-record lance dataset has unexpected null in `{column}`."),
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

fn read_required_u32(array: &Int32Array, index: usize, column: &str) -> Result<u32, CommandError> {
    if array.is_null(index) {
        return Err(CommandError::system_fault(
            "project_record_lance_unexpected_null",
            format!("Xero project-record lance dataset has unexpected null in `{column}`."),
        ));
    }
    u32::try_from(array.value(index)).map_err(|_| {
        CommandError::system_fault(
            "project_record_lance_invalid_u32",
            format!("Xero project-record lance dataset has an invalid u32 in `{column}`."),
        )
    })
}

fn optional_embedding(
    array: &FixedSizeListArray,
    index: usize,
) -> Result<Option<Vec<f32>>, CommandError> {
    if array.is_null(index) {
        return Ok(None);
    }
    if array.value_length() != PROJECT_RECORD_EMBEDDING_DIM {
        return Err(CommandError::system_fault(
            "project_record_lance_embedding_dimension_mismatch",
            format!(
                "Xero project-record Lance embedding dimension is {}; expected {}.",
                array.value_length(),
                PROJECT_RECORD_EMBEDDING_DIM
            ),
        ));
    }
    let values = array.value(index);
    let values = values
        .as_any()
        .downcast_ref::<arrow_array::Float32Array>()
        .ok_or_else(|| missing_column("embedding.item"))?;
    let mut vector = Vec::with_capacity(PROJECT_RECORD_EMBEDDING_DIM as usize);
    for value_index in 0..PROJECT_RECORD_EMBEDDING_DIM as usize {
        if values.is_null(value_index) {
            return Ok(None);
        }
        vector.push(values.value(value_index));
    }
    Ok(Some(vector))
}

fn parse_runtime_agent_id(value: &str) -> RuntimeAgentIdDto {
    match value {
        "plan" => RuntimeAgentIdDto::Plan,
        "engineer" => RuntimeAgentIdDto::Engineer,
        "debug" => RuntimeAgentIdDto::Debug,
        "crawl" => RuntimeAgentIdDto::Crawl,
        "agent_create" => RuntimeAgentIdDto::AgentCreate,
        _ => RuntimeAgentIdDto::Ask,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_record_row(record_id: &str) -> ProjectRecordRow {
        ProjectRecordRow {
            record_id: record_id.into(),
            project_id: "project-1".into(),
            record_kind: "decision".into(),
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "engineer".into(),
            agent_definition_version: 1,
            agent_session_id: Some("default".into()),
            run_id: format!("{record_id}-run"),
            workflow_run_id: None,
            workflow_step_id: None,
            title: "Schema reset record".into(),
            summary: "Schema reset record summary".into(),
            text: "Schema reset record text.".into(),
            text_hash: format!("{record_id}-text-hash"),
            content_json: None,
            content_hash: None,
            schema_name: None,
            schema_version: 1,
            importance: "normal".into(),
            confidence: None,
            tags_json: "[]".into(),
            source_item_ids_json: "[]".into(),
            related_paths_json: "[]".into(),
            produced_artifact_refs_json: "[]".into(),
            redaction_state: "clean".into(),
            visibility: "retrieval".into(),
            freshness_state: "current".into(),
            freshness_checked_at: Some("2026-05-05T16:40:32Z".into()),
            stale_reason: None,
            source_fingerprints_json: "{}".into(),
            supersedes_id: None,
            superseded_by_id: None,
            invalidated_at: None,
            fact_key: None,
            created_at: "2026-05-05T16:40:32Z".into(),
            updated_at: "2026-05-05T16:40:32Z".into(),
            embedding: None,
            embedding_model: None,
            embedding_dimension: None,
            embedding_version: None,
        }
    }

    fn embedded_project_record_row(index: usize) -> ProjectRecordRow {
        let record_id = format!("s34-record-{index:03}");
        let mut row = project_record_row(&record_id);
        row.title = format!("S34 indexed record {index}");
        row.summary = format!("S34 summary {index}");
        row.text = format!("S34 vector retrieval record {index} release blocker context");
        row.text_hash = format!("{:064x}", index + 1);
        row.created_at = format!("2026-05-05T16:{:02}:32Z", index % 60);
        row.updated_at = row.created_at.clone();
        row.importance = if index % 3 == 0 {
            "high".into()
        } else {
            "normal".into()
        };
        if index % 17 == 0 {
            row.redaction_state = "blocked".into();
        }
        let embedding = crate::db::project_store::embedding_with_service(
            &crate::db::project_store::LocalHashEmbeddingService,
            &row.text,
        )
        .expect("test embedding");
        row.embedding = Some(embedding.vector);
        row.embedding_model = Some(embedding.model);
        row.embedding_dimension = Some(embedding.dimension);
        row.embedding_version = Some(embedding.version);
        row
    }

    fn insert_project_record_rows(
        dataset_dir: &Path,
        rows: &[ProjectRecordRow],
    ) -> Result<(), CommandError> {
        runtime().block_on(async {
            let connection = ensure_connection(dataset_dir).await?;
            let table = open_or_create_table(&connection, dataset_dir).await?;
            let batch = build_batch(rows)?;
            table
                .add(vec![batch])
                .execute()
                .await
                .map_err(|error| map_lance_error("test_project_record_lance_insert_failed", error))
                .map(|_| ())
        })
    }

    fn legacy_schema_missing_freshness_state() -> SchemaRef {
        Arc::new(Schema::new(vec![Field::new(
            "record_id",
            DataType::Utf8,
            false,
        )]))
    }

    #[test]
    fn s36_stale_lance_schema_is_quarantined_before_listing_and_insert() {
        reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let dataset_dir = tempdir.path().join(PROJECT_LANCE_SUBDIR);

        runtime()
            .block_on(async {
                let connection = connect_dataset(&dataset_dir).await?;
                let legacy_schema = legacy_schema_missing_freshness_state();
                let empty = RecordBatch::new_empty(legacy_schema.clone());
                let iter = RecordBatchIterator::new(
                    std::iter::once(Ok::<_, arrow_schema::ArrowError>(empty)),
                    legacy_schema,
                );
                let reader: Box<dyn arrow_array::RecordBatchReader + Send + 'static> =
                    Box::new(iter);
                connection
                    .create_table(PROJECT_RECORDS_TABLE, reader)
                    .execute()
                    .await
                    .map_err(|error| map_lance_error("test_legacy_lance_create_failed", error))?;
                Ok::<_, CommandError>(())
            })
            .expect("create legacy lance table");

        let store = ProjectRecordStore {
            project_id: "project-1".into(),
            dataset_dir: dataset_dir.clone(),
        };

        let rows = store.list().expect("list quarantines stale schema");
        assert!(rows.is_empty());
        let quarantine_table = std::fs::read_dir(&dataset_dir)
            .expect("read lance dataset")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| {
                name.starts_with("project_records_quarantine_") && name.ends_with(".lance")
            })
            .expect("quarantined project-record table");
        let quarantine_name = quarantine_table.trim_end_matches(".lance");
        assert!(dataset_dir.join("project_records.lance").exists());
        assert!(dataset_dir
            .join(format!("{quarantine_name}.schema-drift.json"))
            .exists());

        let inserted = store
            .insert_dedup(project_record_row("schema-quarantine-record"))
            .expect("insert after schema quarantine");
        assert_eq!(inserted.record_id, "schema-quarantine-record");

        let rows = store.list().expect("list inserted record");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].freshness_state, "current");

        let health = store.health_report().expect("project-record health report");
        assert_eq!(health.table_name, PROJECT_RECORDS_TABLE);
        assert_eq!(health.status, "degraded");
        assert!(health.schema_current);
        assert_eq!(health.row_count, 1);
        assert_eq!(health.quarantine_table_count, 1);
        assert_eq!(health.diagnostic_marker_count, 1);

        let optimization = store
            .optimize_for_maintenance()
            .expect("project-record optimize");
        assert_eq!(optimization.table_name, PROJECT_RECORDS_TABLE);
        assert!(optimization.after.schema_current);
    }

    #[test]
    fn s37_project_record_health_reports_freshness_counts_before_and_after_maintenance() {
        reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let dataset_dir = tempdir.path().join(PROJECT_LANCE_SUBDIR);
        let store = ProjectRecordStore {
            project_id: "project-1".into(),
            dataset_dir,
        };
        store
            .insert_dedup(project_record_row("current-record"))
            .expect("insert current record");
        let mut stale = project_record_row("stale-record");
        stale.freshness_state = "stale".into();
        stale.stale_reason = Some("Source file changed.".into());
        stale.invalidated_at = Some("2026-05-05T16:45:00Z".into());
        store.insert_dedup(stale).expect("insert stale record");

        let health = store.health_report().expect("project-record health report");
        assert_eq!(health.freshness_counts.inspected_row_count, 2);
        assert_eq!(health.freshness_counts.current_row_count, 1);
        assert_eq!(health.freshness_counts.stale_row_count, 1);
        assert_eq!(health.freshness_counts.retrieval_degraded_row_count(), 1);

        let optimization = store
            .optimize_for_maintenance()
            .expect("project-record optimize");
        assert_eq!(optimization.before.freshness_counts.stale_row_count, 1);
        assert_eq!(optimization.after.freshness_counts.stale_row_count, 1);
    }

    #[test]
    fn s34_project_record_vector_search_creates_index_and_bounds_large_store() {
        reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let dataset_dir = tempdir.path().join(PROJECT_LANCE_SUBDIR);
        let store = ProjectRecordStore {
            project_id: "project-s34-index".into(),
            dataset_dir: dataset_dir.clone(),
        };
        let rows = (0..128)
            .map(embedded_project_record_row)
            .collect::<Vec<_>>();
        insert_project_record_rows(&dataset_dir, &rows).expect("insert s34 records");
        let query = crate::db::project_store::embedding_with_service(
            &crate::db::project_store::LocalHashEmbeddingService,
            "release blocker context",
        )
        .unwrap();

        let started = std::time::Instant::now();
        let results = store
            .vector_search_rows(
                &query.vector,
                24,
                Some("importance = 'high' AND redaction_state <> 'blocked'"),
            )
            .expect("bounded vector search");

        assert!(
            started.elapsed().as_secs() < 20,
            "vector search should stay comfortably bounded for the fixture"
        );
        assert!(results.len() <= 24);
        assert!(results
            .iter()
            .all(|row| row.importance == "high" && row.redaction_state != "blocked"));

        let health = store.health_report().expect("project-record health report");
        assert_eq!(health.row_count, 128);
        assert!(health.index_count >= 1);
    }

    #[test]
    fn s42_unversioned_project_record_replace_is_rejected_without_losing_old_row() {
        reset_connection_cache_for_tests();
        let tempdir = tempfile::tempdir().expect("temp dir");
        let database_path = tempdir.path().join("state.db");
        let store = open_for_database_path(&database_path, "project-safe-replace");
        let inserted = store
            .insert_dedup(project_record_row("safe-replace-record"))
            .expect("insert project record");

        let error = store
            .update_embedding(
                &inserted.record_id,
                vec![0.5; PROJECT_RECORD_EMBEDDING_DIM as usize],
                "local_hash".into(),
                PROJECT_RECORD_EMBEDDING_DIM,
                "v1".into(),
                inserted.updated_at.clone(),
            )
            .expect_err("unversioned replace rejected");
        assert_eq!(
            error.code,
            "project_record_lance_replace_version_not_advanced"
        );

        let rows = store.list().expect("list after rejected replace");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].record_id, inserted.record_id);
        assert!(rows[0].embedding.is_none());
    }
}
