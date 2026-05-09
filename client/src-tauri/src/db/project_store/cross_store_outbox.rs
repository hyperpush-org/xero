use std::path::Path;

use rusqlite::{params, OptionalExtension, Row};
use serde::de::DeserializeOwned;
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{
    commands::CommandError,
    db::{database_path_for_repo, project_store::open_runtime_database},
};

use super::{agent_memory_lance, project_record_lance, read_project_row, validate_non_empty_text};

#[derive(Debug, Clone, PartialEq)]
pub struct CrossStoreOutboxRecord {
    pub id: i64,
    pub operation_id: String,
    pub project_id: String,
    pub store_kind: String,
    pub entity_kind: String,
    pub entity_id: String,
    pub operation: String,
    pub status: String,
    pub payload: JsonValue,
    pub diagnostic: Option<JsonValue>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewCrossStoreOutboxRecord {
    pub operation_id: String,
    pub project_id: String,
    pub store_kind: String,
    pub entity_kind: String,
    pub entity_id: String,
    pub operation: String,
    pub payload: JsonValue,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossStoreReconciliationReport {
    pub project_id: String,
    pub checked_at: String,
    pub inspected_count: usize,
    pub reconciled_count: usize,
    pub failed_count: usize,
    pub diagnostics: Vec<CrossStoreReconciliationDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrossStoreReconciliationDiagnostic {
    pub operation_id: String,
    pub entity_id: String,
    pub store_kind: String,
    pub entity_kind: String,
    pub status_before: String,
    pub status_after: String,
    pub message: String,
}

pub fn cross_store_outbox_operation_id(
    project_id: &str,
    store_kind: &str,
    entity_kind: &str,
    entity_id: &str,
    operation: &str,
    source_hash: &str,
) -> String {
    let mut hasher = Sha256::new();
    for value in [
        project_id,
        store_kind,
        entity_kind,
        entity_id,
        operation,
        source_hash,
    ] {
        hasher.update(value.as_bytes());
        hasher.update(b"\0");
    }
    let hash = format!("{:x}", hasher.finalize());
    format!("cross-store-{}", &hash[..16])
}

pub fn begin_cross_store_outbox_operation(
    repo_root: &Path,
    record: &NewCrossStoreOutboxRecord,
) -> Result<CrossStoreOutboxRecord, CommandError> {
    validate_new_outbox_record(record)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &record.project_id)?;
    let payload_json = serde_json::to_string(&record.payload).map_err(|error| {
        CommandError::system_fault(
            "cross_store_outbox_payload_serialize_failed",
            format!("Xero could not serialize cross-store outbox payload: {error}"),
        )
    })?;
    connection
        .execute(
            r#"
            INSERT INTO cross_store_outbox (
                operation_id,
                project_id,
                store_kind,
                entity_kind,
                entity_id,
                operation,
                status,
                payload_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'pending', ?7, ?8, ?8)
            ON CONFLICT(project_id, operation_id)
            DO UPDATE SET
                status = CASE
                    WHEN cross_store_outbox.status IN ('applied', 'reconciled')
                        THEN cross_store_outbox.status
                    ELSE 'pending'
                END,
                payload_json = excluded.payload_json,
                diagnostic_json = CASE
                    WHEN cross_store_outbox.status IN ('applied', 'reconciled')
                        THEN cross_store_outbox.diagnostic_json
                    ELSE NULL
                END,
                updated_at = excluded.updated_at,
                completed_at = CASE
                    WHEN cross_store_outbox.status IN ('applied', 'reconciled')
                        THEN cross_store_outbox.completed_at
                    ELSE NULL
                END
            "#,
            params![
                record.operation_id,
                record.project_id,
                record.store_kind,
                record.entity_kind,
                record.entity_id,
                record.operation,
                payload_json,
                record.created_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "cross_store_outbox_insert_failed",
                format!(
                    "Xero could not record a cross-store outbox operation in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    get_cross_store_outbox_operation(repo_root, &record.project_id, &record.operation_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "cross_store_outbox_insert_missing",
                "Xero recorded a cross-store outbox operation but could not load it back.",
            )
        })
}

pub fn finish_cross_store_outbox_operation(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    status: &str,
    diagnostic: Option<JsonValue>,
    completed_at: &str,
) -> Result<(), CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "cross_store_outbox_project_required",
    )?;
    validate_non_empty_text(
        operation_id,
        "operationId",
        "cross_store_outbox_operation_required",
    )?;
    validate_outbox_status(status)?;
    validate_non_empty_text(
        completed_at,
        "completedAt",
        "cross_store_outbox_completed_at_required",
    )?;
    let diagnostic_json = diagnostic
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| {
            CommandError::system_fault(
                "cross_store_outbox_diagnostic_serialize_failed",
                format!("Xero could not serialize cross-store outbox diagnostic: {error}"),
            )
        })?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    let updated = connection
        .execute(
            r#"
            UPDATE cross_store_outbox
            SET status = ?3,
                diagnostic_json = ?4,
                updated_at = ?5,
                completed_at = ?5
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![
                project_id,
                operation_id,
                status,
                diagnostic_json,
                completed_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "cross_store_outbox_update_failed",
                format!(
                    "Xero could not update cross-store outbox operation `{operation_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    if updated == 0 {
        return Err(CommandError::system_fault(
            "cross_store_outbox_update_missing",
            format!("Xero could not find cross-store outbox operation `{operation_id}`."),
        ));
    }
    Ok(())
}

pub fn get_cross_store_outbox_operation(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
) -> Result<Option<CrossStoreOutboxRecord>, CommandError> {
    let connection = open_runtime_database(repo_root, &database_path_for_repo(repo_root))?;
    connection
        .query_row(
            cross_store_outbox_select_sql("WHERE project_id = ?1 AND operation_id = ?2").as_str(),
            params![project_id, operation_id],
            read_cross_store_outbox_row,
        )
        .optional()
        .map_err(map_outbox_read_error)?
        .transpose()
}

pub fn list_cross_store_outbox_by_status(
    repo_root: &Path,
    project_id: &str,
    status: &str,
) -> Result<Vec<CrossStoreOutboxRecord>, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "cross_store_outbox_project_required",
    )?;
    validate_outbox_status(status)?;
    let connection = open_runtime_database(repo_root, &database_path_for_repo(repo_root))?;
    let mut statement = connection
        .prepare(
            cross_store_outbox_select_sql(
                "WHERE project_id = ?1 AND status = ?2 ORDER BY created_at ASC, id ASC",
            )
            .as_str(),
        )
        .map_err(map_outbox_read_error)?;
    let rows = statement
        .query_map(params![project_id, status], read_cross_store_outbox_row)
        .map_err(map_outbox_read_error)?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(map_outbox_read_error)?
        .into_iter()
        .collect()
}

pub fn reconcile_cross_store_outbox(
    repo_root: &Path,
    project_id: &str,
    checked_at: &str,
) -> Result<CrossStoreReconciliationReport, CommandError> {
    validate_non_empty_text(
        project_id,
        "projectId",
        "cross_store_outbox_project_required",
    )?;
    validate_non_empty_text(
        checked_at,
        "checkedAt",
        "cross_store_outbox_checked_at_required",
    )?;
    let mut operations = list_cross_store_outbox_by_status(repo_root, project_id, "pending")?;
    operations.extend(list_cross_store_outbox_by_status(
        repo_root, project_id, "failed",
    )?);

    let mut report = CrossStoreReconciliationReport {
        project_id: project_id.to_string(),
        checked_at: checked_at.to_string(),
        inspected_count: operations.len(),
        reconciled_count: 0,
        failed_count: 0,
        diagnostics: Vec::new(),
    };

    for operation in operations {
        let status_before = operation.status.clone();
        match cross_store_entity_exists(repo_root, &operation) {
            Ok(true) => {
                finish_cross_store_outbox_operation(
                    repo_root,
                    project_id,
                    &operation.operation_id,
                    "reconciled",
                    Some(serde_json::json!({
                        "entityFound": true,
                        "reconciledAt": checked_at,
                    })),
                    checked_at,
                )?;
                report.reconciled_count += 1;
                report.diagnostics.push(CrossStoreReconciliationDiagnostic {
                    operation_id: operation.operation_id,
                    entity_id: operation.entity_id,
                    store_kind: operation.store_kind,
                    entity_kind: operation.entity_kind,
                    status_before,
                    status_after: "reconciled".into(),
                    message: "Lance side effect already exists; outbox marked reconciled.".into(),
                });
            }
            Ok(false) => match replay_cross_store_operation(repo_root, &operation) {
                Ok(replayed_id) => {
                    finish_cross_store_outbox_operation(
                        repo_root,
                        project_id,
                        &operation.operation_id,
                        "reconciled",
                        Some(serde_json::json!({
                            "entityFound": false,
                            "replayed": true,
                            "replayedId": replayed_id,
                            "reconciledAt": checked_at,
                        })),
                        checked_at,
                    )?;
                    report.reconciled_count += 1;
                    report.diagnostics.push(CrossStoreReconciliationDiagnostic {
                        operation_id: operation.operation_id,
                        entity_id: operation.entity_id,
                        store_kind: operation.store_kind,
                        entity_kind: operation.entity_kind,
                        status_before,
                        status_after: "reconciled".into(),
                        message: "Missing Lance side effect was replayed from the outbox.".into(),
                    });
                }
                Err(error) => {
                    finish_cross_store_outbox_operation(
                        repo_root,
                        project_id,
                        &operation.operation_id,
                        "failed",
                        Some(serde_json::json!({
                            "code": error.code.clone(),
                            "message": error.message.clone(),
                            "entityFound": false,
                            "reconciledAt": checked_at,
                        })),
                        checked_at,
                    )?;
                    report.failed_count += 1;
                    report.diagnostics.push(CrossStoreReconciliationDiagnostic {
                        operation_id: operation.operation_id,
                        entity_id: operation.entity_id,
                        store_kind: operation.store_kind,
                        entity_kind: operation.entity_kind,
                        status_before,
                        status_after: "failed".into(),
                        message: error.message,
                    });
                }
            },
            Err(error) => {
                finish_cross_store_outbox_operation(
                    repo_root,
                    project_id,
                    &operation.operation_id,
                    "failed",
                    Some(serde_json::json!({
                        "code": error.code.clone(),
                        "message": error.message.clone(),
                        "reconciledAt": checked_at,
                    })),
                    checked_at,
                )?;
                report.failed_count += 1;
                report.diagnostics.push(CrossStoreReconciliationDiagnostic {
                    operation_id: operation.operation_id,
                    entity_id: operation.entity_id,
                    store_kind: operation.store_kind,
                    entity_kind: operation.entity_kind,
                    status_before,
                    status_after: "failed".into(),
                    message: error.message,
                });
            }
        }
    }

    Ok(report)
}

fn replay_cross_store_operation(
    repo_root: &Path,
    operation: &CrossStoreOutboxRecord,
) -> Result<String, CommandError> {
    if operation.operation != "insert" {
        return Err(CommandError::system_fault(
            "cross_store_outbox_replay_unsupported",
            format!(
                "Xero cannot replay cross-store operation `{}` with operation kind `{}`.",
                operation.operation_id, operation.operation
            ),
        ));
    }

    let database_path = database_path_for_repo(repo_root);
    match (operation.store_kind.as_str(), operation.entity_kind.as_str()) {
        ("project_record_lance", "project_record") => {
            let row: project_record_lance::ProjectRecordRow = decode_outbox_row(operation)?;
            let store =
                project_record_lance::open_for_database_path(&database_path, &operation.project_id);
            let inserted = store.insert_dedup(row)?;
            Ok(inserted.record_id)
        }
        ("agent_memory_lance", "approved_memory") => {
            let row: agent_memory_lance::AgentMemoryRow = decode_outbox_row(operation)?;
            let store =
                agent_memory_lance::open_for_database_path(&database_path, &operation.project_id);
            let inserted = store.insert(row)?;
            Ok(inserted.memory_id)
        }
        _ => Err(CommandError::system_fault(
            "cross_store_outbox_replay_unsupported",
            format!(
                "Xero cannot replay cross-store operation `{}` for store `{}` and entity kind `{}`.",
                operation.operation_id, operation.store_kind, operation.entity_kind
            ),
        )),
    }
}

fn decode_outbox_row<T: DeserializeOwned>(
    operation: &CrossStoreOutboxRecord,
) -> Result<T, CommandError> {
    let row = operation.payload.get("row").ok_or_else(|| {
        CommandError::system_fault(
            "cross_store_outbox_payload_missing_row",
            format!(
                "Cross-store operation `{}` does not contain a replayable row payload.",
                operation.operation_id
            ),
        )
    })?;
    serde_json::from_value(row.clone()).map_err(|error| {
        CommandError::system_fault(
            "cross_store_outbox_payload_decode_failed",
            format!(
                "Xero could not decode cross-store operation `{}` for replay: {error}",
                operation.operation_id
            ),
        )
    })
}

fn cross_store_entity_exists(
    repo_root: &Path,
    operation: &CrossStoreOutboxRecord,
) -> Result<bool, CommandError> {
    let database_path = database_path_for_repo(repo_root);
    match (operation.store_kind.as_str(), operation.entity_kind.as_str()) {
        ("project_record_lance", "project_record") => {
            let store =
                project_record_lance::open_for_database_path(&database_path, &operation.project_id);
            Ok(store
                .list()?
                .into_iter()
                .any(|row| row.record_id == operation.entity_id))
        }
        ("agent_memory_lance", "approved_memory") => {
            let store =
                agent_memory_lance::open_for_database_path(&database_path, &operation.project_id);
            Ok(store.get_by_memory_id(&operation.entity_id)?.is_some())
        }
        _ => Err(CommandError::system_fault(
            "cross_store_outbox_reconcile_unsupported",
            format!(
                "Xero cannot reconcile cross-store operation `{}` for store `{}` and entity kind `{}`.",
                operation.operation_id, operation.store_kind, operation.entity_kind
            ),
        )),
    }
}

fn validate_new_outbox_record(record: &NewCrossStoreOutboxRecord) -> Result<(), CommandError> {
    validate_non_empty_text(
        &record.operation_id,
        "operationId",
        "cross_store_outbox_operation_required",
    )?;
    validate_non_empty_text(
        &record.project_id,
        "projectId",
        "cross_store_outbox_project_required",
    )?;
    validate_non_empty_text(
        &record.store_kind,
        "storeKind",
        "cross_store_outbox_store_required",
    )?;
    validate_non_empty_text(
        &record.entity_kind,
        "entityKind",
        "cross_store_outbox_entity_kind_required",
    )?;
    validate_non_empty_text(
        &record.entity_id,
        "entityId",
        "cross_store_outbox_entity_required",
    )?;
    validate_non_empty_text(
        &record.operation,
        "operation",
        "cross_store_outbox_operation_kind_required",
    )?;
    validate_non_empty_text(
        &record.created_at,
        "createdAt",
        "cross_store_outbox_created_at_required",
    )?;
    Ok(())
}

fn validate_outbox_status(status: &str) -> Result<(), CommandError> {
    if matches!(status, "pending" | "applied" | "failed" | "reconciled") {
        Ok(())
    } else {
        Err(CommandError::invalid_request("status"))
    }
}

fn cross_store_outbox_select_sql(where_clause: &str) -> String {
    format!(
        r#"
        SELECT
            id,
            operation_id,
            project_id,
            store_kind,
            entity_kind,
            entity_id,
            operation,
            status,
            payload_json,
            diagnostic_json,
            created_at,
            updated_at,
            completed_at
        FROM cross_store_outbox
        {where_clause}
        "#
    )
}

fn read_cross_store_outbox_row(
    row: &Row<'_>,
) -> rusqlite::Result<Result<CrossStoreOutboxRecord, CommandError>> {
    let payload_json: String = row.get(8)?;
    let diagnostic_json: Option<String> = row.get(9)?;
    Ok(Ok(CrossStoreOutboxRecord {
        id: row.get(0)?,
        operation_id: row.get(1)?,
        project_id: row.get(2)?,
        store_kind: row.get(3)?,
        entity_kind: row.get(4)?,
        entity_id: row.get(5)?,
        operation: row.get(6)?,
        status: row.get(7)?,
        payload: serde_json::from_str(&payload_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                8,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?,
        diagnostic: diagnostic_json
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    9,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?,
        created_at: row.get(10)?,
        updated_at: row.get(11)?,
        completed_at: row.get(12)?,
    }))
}

fn map_outbox_read_error(error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        "cross_store_outbox_read_failed",
        format!("Xero could not read cross-store outbox state: {error}"),
    )
}
