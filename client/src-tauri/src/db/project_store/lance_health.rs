use std::{
    fs,
    path::{Path, PathBuf},
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use arrow_schema::Schema;
use lancedb::{
    index::{vector::IvfFlatIndexBuilder, Index, IndexType},
    table::{OptimizeAction, OptimizeStats, TableStatistics},
    DistanceType, Table,
};
use serde_json::json;

use crate::commands::CommandError;

const VECTOR_INDEX_MIN_NON_NULL_ROWS: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LanceTableHealthReport {
    pub table_name: String,
    pub status: String,
    pub schema_current: bool,
    pub version: u64,
    pub row_count: usize,
    pub total_bytes: usize,
    pub index_count: usize,
    pub fragment_count: usize,
    pub small_fragment_count: usize,
    pub stats_latency_ms: u64,
    pub maintenance_recommended: bool,
    pub quarantine_table_count: usize,
    pub diagnostic_marker_count: usize,
    pub freshness_counts: LanceTableFreshnessCounts,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LanceTableOptimizationReport {
    pub table_name: String,
    pub before: LanceTableHealthReport,
    pub after: LanceTableHealthReport,
    pub compaction: Option<LanceCompactionMetrics>,
    pub prune: Option<LancePruneMetrics>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LanceVectorIndexMaintenanceReport {
    pub table_name: String,
    pub index_name: String,
    pub vector_column: String,
    pub non_null_vector_rows: usize,
    pub existed_before: bool,
    pub created: bool,
    pub optimized: bool,
    pub indexed_rows: Option<usize>,
    pub unindexed_rows: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LanceCompactionMetrics {
    pub fragments_removed: usize,
    pub fragments_added: usize,
    pub files_removed: usize,
    pub files_added: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LancePruneMetrics {
    pub bytes_removed: u64,
    pub old_versions: u64,
    pub data_files_removed: u64,
    pub transaction_files_removed: u64,
    pub index_files_removed: u64,
    pub deletion_files_removed: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct LanceTableFreshnessCounts {
    pub inspected_row_count: usize,
    pub current_row_count: usize,
    pub source_unknown_row_count: usize,
    pub stale_row_count: usize,
    pub source_missing_row_count: usize,
    pub superseded_row_count: usize,
    pub blocked_row_count: usize,
}

impl LanceTableFreshnessCounts {
    pub(crate) fn retrieval_degraded_row_count(&self) -> usize {
        self.stale_row_count
            + self.source_missing_row_count
            + self.superseded_row_count
            + self.blocked_row_count
    }
}

pub(crate) fn freshness_counts_from_states<'a>(
    states: impl IntoIterator<Item = &'a str>,
) -> LanceTableFreshnessCounts {
    let mut counts = LanceTableFreshnessCounts::default();
    for state in states {
        counts.inspected_row_count += 1;
        match state {
            "current" => counts.current_row_count += 1,
            "stale" => counts.stale_row_count += 1,
            "source_missing" => counts.source_missing_row_count += 1,
            "superseded" => counts.superseded_row_count += 1,
            "blocked" => counts.blocked_row_count += 1,
            _ => counts.source_unknown_row_count += 1,
        }
    }
    counts
}

pub(crate) fn table_schema_supports_expected(
    table_schema: &Schema,
    expected_schema: &Schema,
) -> bool {
    expected_schema.fields().iter().all(|expected| {
        let Ok(actual) = table_schema.field_with_name(expected.name()) else {
            return false;
        };
        actual.data_type() == expected.data_type() && actual.is_nullable() == expected.is_nullable()
    })
}

pub(crate) fn schema_drift_error(
    code: &'static str,
    store_label: &str,
    column: &str,
) -> CommandError {
    CommandError::system_fault(
        code,
        format!("Xero {store_label} Lance dataset is missing expected column `{column}`."),
    )
}

pub(crate) async fn table_health_report(
    table: &Table,
    dataset_dir: &Path,
    table_name: &str,
    schema_current: bool,
) -> Result<LanceTableHealthReport, CommandError> {
    let started = Instant::now();
    let stats = table.stats().await.map_err(|error| {
        CommandError::retryable(
            "lance_table_health_stats_failed",
            format!("Xero could not read Lance table `{table_name}` statistics: {error}"),
        )
    })?;
    let stats_latency_ms = started.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let version = table.version().await.map_err(|error| {
        CommandError::retryable(
            "lance_table_health_version_failed",
            format!("Xero could not read Lance table `{table_name}` version: {error}"),
        )
    })?;
    let (quarantine_table_count, diagnostic_marker_count) =
        quarantine_artifact_counts(dataset_dir, table_name)?;
    Ok(table_health_report_from_stats(
        table_name,
        schema_current,
        version,
        stats,
        stats_latency_ms,
        quarantine_table_count,
        diagnostic_marker_count,
    ))
}

pub(crate) fn table_optimization_report(
    table_name: &str,
    before: LanceTableHealthReport,
    stats: &OptimizeStats,
    after: LanceTableHealthReport,
) -> LanceTableOptimizationReport {
    LanceTableOptimizationReport {
        table_name: table_name.to_string(),
        before,
        after,
        compaction: stats
            .compaction
            .as_ref()
            .map(|compaction| LanceCompactionMetrics {
                fragments_removed: compaction.fragments_removed,
                fragments_added: compaction.fragments_added,
                files_removed: compaction.files_removed,
                files_added: compaction.files_added,
            }),
        prune: stats.prune.as_ref().map(|prune| LancePruneMetrics {
            bytes_removed: prune.bytes_removed,
            old_versions: prune.old_versions,
            data_files_removed: prune.data_files_removed,
            transaction_files_removed: prune.transaction_files_removed,
            index_files_removed: prune.index_files_removed,
            deletion_files_removed: prune.deletion_files_removed,
        }),
    }
}

pub(crate) async fn maintain_cosine_vector_index(
    table: &Table,
    table_name: &str,
    vector_column: &str,
    index_name: &str,
    optimize: bool,
) -> Result<LanceVectorIndexMaintenanceReport, CommandError> {
    let non_null_vector_rows = table
        .count_rows(Some(format!("{vector_column} IS NOT NULL")))
        .await
        .map_err(|error| {
            CommandError::retryable(
                "lance_vector_index_count_failed",
                format!(
                    "Xero could not count non-null vector rows for Lance table `{table_name}` column `{vector_column}`: {error}"
                ),
            )
        })?;
    let indices = table.list_indices().await.map_err(|error| {
        CommandError::retryable(
            "lance_vector_index_list_failed",
            format!("Xero could not list Lance indices for table `{table_name}`: {error}"),
        )
    })?;
    let existing = indices.into_iter().find(|index| {
        index.columns.iter().any(|column| column == vector_column)
            && is_vector_index_type(&index.index_type)
    });
    let existed_before = existing.is_some();
    let actual_index_name = existing
        .map(|index| index.name)
        .unwrap_or_else(|| index_name.to_string());

    let mut created = false;
    if !existed_before && non_null_vector_rows >= VECTOR_INDEX_MIN_NON_NULL_ROWS {
        table
            .create_index(
                &[vector_column],
                Index::IvfFlat(
                    IvfFlatIndexBuilder::default()
                        .distance_type(DistanceType::Cosine)
                        .target_partition_size(1024),
                ),
            )
            .name(actual_index_name.clone())
            .replace(false)
            .execute()
            .await
            .map_err(|error| {
                CommandError::retryable(
                    "lance_vector_index_create_failed",
                    format!(
                        "Xero could not create cosine vector index `{actual_index_name}` on Lance table `{table_name}` column `{vector_column}`: {error}"
                    ),
                )
            })?;
        created = true;
    }

    let mut optimized = false;
    if optimize && (existed_before || created) {
        table
            .optimize(OptimizeAction::Index(Default::default()))
            .await
            .map_err(|error| {
                CommandError::retryable(
                    "lance_vector_index_optimize_failed",
                    format!(
                        "Xero could not optimize vector index `{actual_index_name}` on Lance table `{table_name}`: {error}"
                    ),
                )
            })?;
        optimized = true;
    }

    let (indexed_rows, unindexed_rows) = if existed_before || created {
        match table.index_stats(&actual_index_name).await.map_err(|error| {
            CommandError::retryable(
                "lance_vector_index_stats_failed",
                format!(
                    "Xero could not read vector index stats for `{actual_index_name}` on Lance table `{table_name}`: {error}"
                ),
            )
        })? {
            Some(stats) => (Some(stats.num_indexed_rows), Some(stats.num_unindexed_rows)),
            None => (None, None),
        }
    } else {
        (None, None)
    };

    Ok(LanceVectorIndexMaintenanceReport {
        table_name: table_name.to_string(),
        index_name: actual_index_name,
        vector_column: vector_column.to_string(),
        non_null_vector_rows,
        existed_before,
        created,
        optimized,
        indexed_rows,
        unindexed_rows,
    })
}

fn is_vector_index_type(index_type: &IndexType) -> bool {
    matches!(
        index_type,
        IndexType::IvfFlat
            | IndexType::IvfPq
            | IndexType::IvfSq
            | IndexType::IvfRq
            | IndexType::IvfHnswPq
            | IndexType::IvfHnswSq
    )
}

fn table_health_report_from_stats(
    table_name: &str,
    schema_current: bool,
    version: u64,
    stats: TableStatistics,
    stats_latency_ms: u64,
    quarantine_table_count: usize,
    diagnostic_marker_count: usize,
) -> LanceTableHealthReport {
    let fragment_count = stats.fragment_stats.num_fragments;
    let small_fragment_count = stats.fragment_stats.num_small_fragments;
    let maintenance_recommended = small_fragment_count > 20;
    let status = if !schema_current {
        "schema_drift"
    } else if quarantine_table_count > 0 || maintenance_recommended {
        "degraded"
    } else {
        "healthy"
    };
    LanceTableHealthReport {
        table_name: table_name.to_string(),
        status: status.to_string(),
        schema_current,
        version,
        row_count: stats.num_rows,
        total_bytes: stats.total_bytes,
        index_count: stats.num_indices,
        fragment_count,
        small_fragment_count,
        stats_latency_ms,
        maintenance_recommended,
        quarantine_table_count,
        diagnostic_marker_count,
        freshness_counts: LanceTableFreshnessCounts::default(),
    }
}

pub(crate) fn quarantine_lance_table_directory(
    dataset_dir: &Path,
    table_name: &str,
    store_label: &str,
    reason_code: &'static str,
) -> Result<Option<String>, CommandError> {
    let source_path = lance_table_path(dataset_dir, table_name);
    if !source_path.try_exists().map_err(|error| {
        CommandError::retryable(
            "lance_schema_quarantine_inspect_failed",
            format!(
                "Xero could not inspect {store_label} Lance table `{table_name}` for quarantine at {}: {error}",
                source_path.display()
            ),
        )
    })? {
        return Ok(None);
    }

    let stamp = quarantine_timestamp_millis();
    for attempt in 0..100_u32 {
        let quarantine_table = if attempt == 0 {
            format!("{table_name}_quarantine_{stamp}")
        } else {
            format!("{table_name}_quarantine_{stamp}_{attempt}")
        };
        let quarantine_path = lance_table_path(dataset_dir, &quarantine_table);
        if quarantine_path.exists() {
            continue;
        }
        fs::rename(&source_path, &quarantine_path).map_err(|error| {
            CommandError::retryable(
                "lance_schema_quarantine_failed",
                format!(
                    "Xero could not quarantine {store_label} Lance table `{table_name}` from {} to {}: {error}",
                    source_path.display(),
                    quarantine_path.display()
                ),
            )
        })?;
        write_quarantine_marker(
            dataset_dir,
            table_name,
            &quarantine_table,
            store_label,
            reason_code,
            stamp,
        );
        return Ok(Some(quarantine_table));
    }

    Err(CommandError::retryable(
        "lance_schema_quarantine_name_exhausted",
        format!(
            "Xero could not find an unused quarantine name for {store_label} Lance table `{table_name}`."
        ),
    ))
}

fn quarantine_artifact_counts(
    dataset_dir: &Path,
    table_name: &str,
) -> Result<(usize, usize), CommandError> {
    if !dataset_dir.try_exists().map_err(|error| {
        CommandError::retryable(
            "lance_schema_quarantine_inspect_failed",
            format!(
                "Xero could not inspect Lance dataset directory {} for quarantine artifacts: {error}",
                dataset_dir.display()
            ),
        )
    })? {
        return Ok((0, 0));
    }
    let prefix = format!("{table_name}_quarantine_");
    let mut quarantine_table_count = 0_usize;
    let mut diagnostic_marker_count = 0_usize;
    for entry in fs::read_dir(dataset_dir).map_err(|error| {
        CommandError::retryable(
            "lance_schema_quarantine_inspect_failed",
            format!(
                "Xero could not read Lance dataset directory {} for quarantine artifacts: {error}",
                dataset_dir.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            CommandError::retryable(
                "lance_schema_quarantine_inspect_failed",
                format!("Xero could not inspect a Lance quarantine artifact: {error}"),
            )
        })?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with(&prefix) && name.ends_with(".lance") {
            quarantine_table_count += 1;
        } else if name.starts_with(&prefix) && name.ends_with(".schema-drift.json") {
            diagnostic_marker_count += 1;
        }
    }
    Ok((quarantine_table_count, diagnostic_marker_count))
}

fn lance_table_path(dataset_dir: &Path, table_name: &str) -> PathBuf {
    dataset_dir.join(format!("{table_name}.lance"))
}

fn quarantine_timestamp_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn write_quarantine_marker(
    dataset_dir: &Path,
    original_table: &str,
    quarantine_table: &str,
    store_label: &str,
    reason_code: &'static str,
    quarantined_at_epoch_ms: u128,
) {
    let marker_path = dataset_dir.join(format!("{quarantine_table}.schema-drift.json"));
    let diagnostic = json!({
        "code": reason_code,
        "storeLabel": store_label,
        "originalTable": original_table,
        "quarantineTable": quarantine_table,
        "quarantinedAtEpochMs": quarantined_at_epoch_ms.to_string(),
        "message": "Xero quarantined a Lance table with an unsupported schema and created a clean replacement table."
    });
    if let Ok(bytes) = serde_json::to_vec_pretty(&diagnostic) {
        let _ = fs::write(marker_path, bytes);
    }
}
