use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use arrow_array::{
    Array, BinaryArray, BooleanArray, Date32Array, Date64Array, Decimal128Array,
    FixedSizeBinaryArray, FixedSizeListArray, Float32Array, Float64Array, Int16Array, Int32Array,
    Int64Array, Int8Array, LargeBinaryArray, LargeStringArray, RecordBatch, StringArray,
    TimestampMicrosecondArray, TimestampMillisecondArray, TimestampNanosecondArray,
    TimestampSecondArray, UInt16Array, UInt32Array, UInt64Array, UInt8Array,
};
use arrow_schema::{DataType, TimeUnit};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine as _};
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use rusqlite::{types::ValueRef, Connection};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use tauri::{AppHandle, Runtime, State};

use crate::{
    commands::{validate_non_empty, CommandError, CommandResult},
    db::{self, project_store::agent_memory_lance},
    global_db::open_global_database,
    registry,
    state::DesktopState,
};

const DEFAULT_ROW_LIMIT: u32 = 50;
const MAX_ROW_LIMIT: u32 = 200;
const VALUE_PREVIEW_LIMIT: usize = 12_000;
const BLOB_PREVIEW_LIMIT: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DeveloperStorageSourceKindDto {
    GlobalSqlite,
    ProjectLance,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperStorageSourceDto {
    pub kind: DeveloperStorageSourceKindDto,
    pub project_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperStorageColumnDto {
    pub name: String,
    pub type_label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperStorageTableSummaryDto {
    pub name: String,
    pub columns: Vec<DeveloperStorageColumnDto>,
    pub row_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperSqliteDatabaseDto {
    pub path: String,
    pub tables: Vec<DeveloperStorageTableSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperProjectLanceDatabaseDto {
    pub project_id: String,
    pub project_name: String,
    pub repository_root: String,
    pub state_database_path: String,
    pub lance_path: String,
    pub exists: bool,
    pub tables: Vec<DeveloperStorageTableSummaryDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperStorageOverviewDto {
    pub global_sqlite: DeveloperSqliteDatabaseDto,
    pub project_lance: Vec<DeveloperProjectLanceDatabaseDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperReadStorageTableRequestDto {
    pub source: DeveloperStorageSourceDto,
    pub table_name: String,
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub reveal_sensitive: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperStorageRowDto {
    pub values: Map<String, Value>,
    pub display_values: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperStorageTableRowsDto {
    pub source: DeveloperStorageSourceDto,
    pub table_name: String,
    pub path: String,
    pub columns: Vec<DeveloperStorageColumnDto>,
    pub rows: Vec<DeveloperStorageRowDto>,
    pub row_count: u64,
    pub limit: u32,
    pub offset: u32,
    pub redacted: bool,
}

#[tauri::command]
pub fn developer_storage_overview<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
) -> CommandResult<DeveloperStorageOverviewDto> {
    let global_db_path = state.global_db_path(&app)?;
    db::configure_project_database_paths(&global_db_path);

    let connection = open_global_database(&global_db_path)?;
    let project_names = read_project_names(&connection)?;
    let global_sqlite = DeveloperSqliteDatabaseDto {
        path: display_path(&global_db_path),
        tables: inspect_sqlite_tables(&connection)?,
    };

    let registry = registry::read_registry(&global_db_path)?;
    let mut project_lance = Vec::with_capacity(registry.projects.len());
    for project in registry.projects {
        let state_database_path = db::database_path_for_project(&project.project_id);
        let lance_path = agent_memory_lance::dataset_dir_for_database_path(&state_database_path);
        let tables = inspect_lance_tables(&lance_path)?;
        let project_name = project_names
            .get(&project.project_id)
            .cloned()
            .unwrap_or_else(|| fallback_project_name(&project.root_path));

        project_lance.push(DeveloperProjectLanceDatabaseDto {
            project_id: project.project_id,
            project_name,
            repository_root: project.root_path,
            state_database_path: display_path(&state_database_path),
            lance_path: display_path(&lance_path),
            exists: lance_path.is_dir(),
            tables,
        });
    }

    Ok(DeveloperStorageOverviewDto {
        global_sqlite,
        project_lance,
    })
}

#[tauri::command]
pub fn developer_storage_read_table<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: DeveloperReadStorageTableRequestDto,
) -> CommandResult<DeveloperStorageTableRowsDto> {
    validate_non_empty(&request.table_name, "tableName")?;

    let limit = normalize_limit(request.limit);
    let offset = request.offset.unwrap_or(0);
    let reveal_sensitive = request.reveal_sensitive.unwrap_or(false);
    let global_db_path = state.global_db_path(&app)?;
    db::configure_project_database_paths(&global_db_path);

    match request.source.kind {
        DeveloperStorageSourceKindDto::GlobalSqlite => {
            let connection = open_global_database(&global_db_path)?;
            read_sqlite_table_rows(
                &connection,
                &global_db_path,
                request.table_name,
                limit,
                offset,
                reveal_sensitive,
            )
        }
        DeveloperStorageSourceKindDto::ProjectLance => {
            let project_id = request
                .source
                .project_id
                .as_deref()
                .ok_or_else(|| CommandError::invalid_request("projectId"))?;
            validate_non_empty(project_id, "projectId")?;
            let state_database_path =
                resolve_project_state_database_path(&global_db_path, project_id)?;
            let lance_path =
                agent_memory_lance::dataset_dir_for_database_path(&state_database_path);
            read_lance_table_rows(
                project_id,
                &lance_path,
                request.table_name,
                limit,
                offset,
                reveal_sensitive,
            )
        }
    }
}

fn inspect_sqlite_tables(
    connection: &Connection,
) -> CommandResult<Vec<DeveloperStorageTableSummaryDto>> {
    let table_names = sqlite_table_names(connection)?;
    let mut tables = Vec::with_capacity(table_names.len());
    for table_name in table_names {
        tables.push(DeveloperStorageTableSummaryDto {
            columns: sqlite_columns(connection, &table_name)?,
            row_count: sqlite_row_count(connection, &table_name)?,
            name: table_name,
        });
    }
    Ok(tables)
}

fn sqlite_table_names(connection: &Connection) -> CommandResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            "SELECT name FROM sqlite_schema \
             WHERE type = 'table' AND name NOT LIKE 'sqlite_%' \
             ORDER BY name",
        )
        .map_err(|error| storage_error("developer_storage_sqlite_schema_failed", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| storage_error("developer_storage_sqlite_schema_failed", error))?;

    let mut names = Vec::new();
    for row in rows {
        names.push(
            row.map_err(|error| storage_error("developer_storage_sqlite_schema_failed", error))?,
        );
    }
    Ok(names)
}

fn sqlite_columns(
    connection: &Connection,
    table_name: &str,
) -> CommandResult<Vec<DeveloperStorageColumnDto>> {
    let query = format!("PRAGMA table_info({})", quote_identifier(table_name));
    let mut statement = connection
        .prepare(&query)
        .map_err(|error| storage_error("developer_storage_sqlite_schema_failed", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok(DeveloperStorageColumnDto {
                name: row.get(1)?,
                type_label: row.get::<_, String>(2).unwrap_or_default(),
            })
        })
        .map_err(|error| storage_error("developer_storage_sqlite_schema_failed", error))?;

    let mut columns = Vec::new();
    for row in rows {
        columns.push(
            row.map_err(|error| storage_error("developer_storage_sqlite_schema_failed", error))?,
        );
    }
    Ok(columns)
}

fn sqlite_row_count(connection: &Connection, table_name: &str) -> CommandResult<u64> {
    let query = format!("SELECT COUNT(*) FROM {}", quote_identifier(table_name));
    let count = connection
        .query_row(&query, [], |row| row.get::<_, i64>(0))
        .map_err(|error| storage_error("developer_storage_sqlite_count_failed", error))?;
    Ok(count.max(0) as u64)
}

fn read_sqlite_table_rows(
    connection: &Connection,
    database_path: &Path,
    table_name: String,
    limit: u32,
    offset: u32,
    reveal_sensitive: bool,
) -> CommandResult<DeveloperStorageTableRowsDto> {
    ensure_sqlite_table_exists(connection, &table_name)?;
    let columns = sqlite_columns(connection, &table_name)?;
    let row_count = sqlite_row_count(connection, &table_name)?;
    let column_names: Vec<String> = columns.iter().map(|column| column.name.clone()).collect();
    let query = format!(
        "SELECT * FROM {} LIMIT ?1 OFFSET ?2",
        quote_identifier(&table_name)
    );
    let mut statement = connection
        .prepare(&query)
        .map_err(|error| storage_error("developer_storage_sqlite_read_failed", error))?;
    let mut rows = statement
        .query(rusqlite::params![limit as i64, offset as i64])
        .map_err(|error| storage_error("developer_storage_sqlite_read_failed", error))?;

    let mut rendered_rows = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| storage_error("developer_storage_sqlite_read_failed", error))?
    {
        let mut values = Map::new();
        for (index, column_name) in column_names.iter().enumerate() {
            let value = if !reveal_sensitive && is_sensitive_cell(&table_name, column_name) {
                json!("[redacted]")
            } else {
                sqlite_value_to_json(row.get_ref(index).map_err(|error| {
                    storage_error("developer_storage_sqlite_read_failed", error)
                })?)
            };
            values.insert(column_name.clone(), value);
        }
        rendered_rows.push(developer_storage_row(values));
    }

    Ok(DeveloperStorageTableRowsDto {
        source: DeveloperStorageSourceDto {
            kind: DeveloperStorageSourceKindDto::GlobalSqlite,
            project_id: None,
        },
        table_name,
        path: display_path(database_path),
        columns,
        rows: rendered_rows,
        row_count,
        limit,
        offset,
        redacted: !reveal_sensitive,
    })
}

fn ensure_sqlite_table_exists(connection: &Connection, table_name: &str) -> CommandResult<()> {
    if sqlite_table_names(connection)?
        .iter()
        .any(|candidate| candidate == table_name)
    {
        Ok(())
    } else {
        Err(CommandError::user_fixable(
            "developer_storage_table_not_found",
            format!("Xero could not find SQLite table `{table_name}` in the global database."),
        ))
    }
}

fn inspect_lance_tables(
    dataset_path: &Path,
) -> CommandResult<Vec<DeveloperStorageTableSummaryDto>> {
    if !dataset_path.is_dir() {
        return Ok(Vec::new());
    }

    let dataset_path = dataset_path.to_path_buf();
    agent_memory_lance::runtime().block_on(async move {
        let connection = connect_lance_dataset(&dataset_path).await?;
        let names = connection
            .table_names()
            .execute()
            .await
            .map_err(|error| storage_error("developer_storage_lance_schema_failed", error))?;
        let mut tables = Vec::with_capacity(names.len());
        for name in names {
            let table = connection
                .open_table(&name)
                .execute()
                .await
                .map_err(|error| storage_error("developer_storage_lance_schema_failed", error))?;
            let schema = table
                .schema()
                .await
                .map_err(|error| storage_error("developer_storage_lance_schema_failed", error))?;
            let columns = schema
                .fields()
                .iter()
                .map(|field| DeveloperStorageColumnDto {
                    name: field.name().to_string(),
                    type_label: data_type_label(field.data_type()),
                })
                .collect();
            let row_count = table
                .count_rows(None)
                .await
                .map_err(|error| storage_error("developer_storage_lance_count_failed", error))?;
            tables.push(DeveloperStorageTableSummaryDto {
                name,
                columns,
                row_count: row_count as u64,
            });
        }
        Ok(tables)
    })
}

fn read_lance_table_rows(
    project_id: &str,
    dataset_path: &Path,
    table_name: String,
    limit: u32,
    offset: u32,
    reveal_sensitive: bool,
) -> CommandResult<DeveloperStorageTableRowsDto> {
    if !dataset_path.is_dir() {
        return Err(CommandError::user_fixable(
            "developer_storage_lance_missing",
            format!(
                "Xero could not find a LanceDB dataset for project `{project_id}` at {}.",
                dataset_path.display()
            ),
        ));
    }

    let dataset_path = dataset_path.to_path_buf();
    let project_id = project_id.to_string();
    agent_memory_lance::runtime().block_on(async move {
        let connection = connect_lance_dataset(&dataset_path).await?;
        let table_names = connection
            .table_names()
            .execute()
            .await
            .map_err(|error| storage_error("developer_storage_lance_schema_failed", error))?;
        if !table_names.iter().any(|candidate| candidate == &table_name) {
            return Err(CommandError::user_fixable(
                "developer_storage_table_not_found",
                format!(
                    "Xero could not find LanceDB table `{table_name}` for project `{project_id}`."
                ),
            ));
        }

        let table = connection
            .open_table(&table_name)
            .execute()
            .await
            .map_err(|error| storage_error("developer_storage_lance_read_failed", error))?;
        let schema = table
            .schema()
            .await
            .map_err(|error| storage_error("developer_storage_lance_schema_failed", error))?;
        let columns: Vec<DeveloperStorageColumnDto> = schema
            .fields()
            .iter()
            .map(|field| DeveloperStorageColumnDto {
                name: field.name().to_string(),
                type_label: data_type_label(field.data_type()),
            })
            .collect();
        let row_count = table
            .count_rows(None)
            .await
            .map_err(|error| storage_error("developer_storage_lance_count_failed", error))?
            as u64;
        let stream = table
            .query()
            .limit(limit as usize)
            .offset(offset as usize)
            .execute()
            .await
            .map_err(|error| storage_error("developer_storage_lance_read_failed", error))?;
        let batches: Vec<RecordBatch> = stream
            .try_collect()
            .await
            .map_err(|error| storage_error("developer_storage_lance_read_failed", error))?;
        let rows = lance_batches_to_rows(&table_name, &columns, batches, reveal_sensitive);

        Ok(DeveloperStorageTableRowsDto {
            source: DeveloperStorageSourceDto {
                kind: DeveloperStorageSourceKindDto::ProjectLance,
                project_id: Some(project_id),
            },
            table_name,
            path: display_path(&dataset_path),
            columns,
            rows,
            row_count,
            limit,
            offset,
            redacted: !reveal_sensitive,
        })
    })
}

async fn connect_lance_dataset(dataset_path: &Path) -> CommandResult<lancedb::Connection> {
    let uri = dataset_path
        .to_str()
        .ok_or_else(|| {
            CommandError::system_fault(
                "developer_storage_lance_path_non_utf8",
                format!(
                    "Xero cannot inspect non-UTF8 LanceDB path {}.",
                    dataset_path.display()
                ),
            )
        })?
        .to_string();
    lancedb::connect(&uri)
        .execute()
        .await
        .map_err(|error| storage_error("developer_storage_lance_connect_failed", error))
}

fn lance_batches_to_rows(
    table_name: &str,
    columns: &[DeveloperStorageColumnDto],
    batches: Vec<RecordBatch>,
    reveal_sensitive: bool,
) -> Vec<DeveloperStorageRowDto> {
    let mut rows = Vec::new();
    for batch in batches {
        for row_index in 0..batch.num_rows() {
            let mut values = Map::new();
            for column in columns {
                let value = if !reveal_sensitive && is_sensitive_cell(table_name, &column.name) {
                    json!("[redacted]")
                } else {
                    batch
                        .column_by_name(&column.name)
                        .map(|array| arrow_value_to_json(array.as_ref(), row_index))
                        .unwrap_or(Value::Null)
                };
                values.insert(column.name.clone(), value);
            }
            rows.push(developer_storage_row(values));
        }
    }
    rows
}

fn developer_storage_row(values: Map<String, Value>) -> DeveloperStorageRowDto {
    let display_values = values
        .iter()
        .map(|(key, value)| (key.clone(), storage_display_value(value)))
        .collect();
    DeveloperStorageRowDto {
        values,
        display_values,
    }
}

fn storage_display_value(value: &Value) -> String {
    let rendered = match value {
        Value::Null => "NULL".to_string(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        Value::Array(_) | Value::Object(_) => {
            serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
        }
    };
    preview_text(&rendered)
}

fn resolve_project_state_database_path(
    global_db_path: &Path,
    project_id: &str,
) -> CommandResult<PathBuf> {
    let registry = registry::read_registry(global_db_path)?;
    let exists = registry
        .projects
        .iter()
        .any(|project| project.project_id == project_id);
    if !exists {
        return Err(CommandError::project_not_found());
    }
    Ok(db::database_path_for_project(project_id))
}

fn read_project_names(connection: &Connection) -> CommandResult<HashMap<String, String>> {
    let mut statement = connection
        .prepare("SELECT id, name FROM projects")
        .map_err(|error| storage_error("developer_storage_project_names_failed", error))?;
    let rows = statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| storage_error("developer_storage_project_names_failed", error))?;
    let mut names = HashMap::new();
    for row in rows {
        let (id, name) =
            row.map_err(|error| storage_error("developer_storage_project_names_failed", error))?;
        names.insert(id, name);
    }
    Ok(names)
}

fn sqlite_value_to_json(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => json!(value),
        ValueRef::Real(value) => finite_f64_json(value),
        ValueRef::Text(bytes) => json!(preview_text(&String::from_utf8_lossy(bytes))),
        ValueRef::Blob(bytes) => json!(blob_preview(bytes)),
    }
}

fn arrow_value_to_json(array: &dyn Array, index: usize) -> Value {
    if array.is_null(index) {
        return Value::Null;
    }

    match array.data_type() {
        DataType::Boolean => json!(array
            .as_any()
            .downcast_ref::<BooleanArray>()
            .map(|array| array.value(index))
            .unwrap_or(false)),
        DataType::Int8 => json!(array
            .as_any()
            .downcast_ref::<Int8Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Int16 => json!(array
            .as_any()
            .downcast_ref::<Int16Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Int32 => json!(array
            .as_any()
            .downcast_ref::<Int32Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Int64 => json!(array
            .as_any()
            .downcast_ref::<Int64Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::UInt8 => json!(array
            .as_any()
            .downcast_ref::<UInt8Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::UInt16 => json!(array
            .as_any()
            .downcast_ref::<UInt16Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::UInt32 => json!(array
            .as_any()
            .downcast_ref::<UInt32Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::UInt64 => json!(array
            .as_any()
            .downcast_ref::<UInt64Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Float32 => array
            .as_any()
            .downcast_ref::<Float32Array>()
            .map(|array| finite_f64_json(array.value(index) as f64))
            .unwrap_or(Value::Null),
        DataType::Float64 => array
            .as_any()
            .downcast_ref::<Float64Array>()
            .map(|array| finite_f64_json(array.value(index)))
            .unwrap_or(Value::Null),
        DataType::Utf8 => json!(array
            .as_any()
            .downcast_ref::<StringArray>()
            .map(|array| preview_text(array.value(index)))
            .unwrap_or_default()),
        DataType::LargeUtf8 => json!(array
            .as_any()
            .downcast_ref::<LargeStringArray>()
            .map(|array| preview_text(array.value(index)))
            .unwrap_or_default()),
        DataType::Binary => json!(array
            .as_any()
            .downcast_ref::<BinaryArray>()
            .map(|array| blob_preview(array.value(index)))
            .unwrap_or_else(|| "[binary]".to_string())),
        DataType::LargeBinary => json!(array
            .as_any()
            .downcast_ref::<LargeBinaryArray>()
            .map(|array| blob_preview(array.value(index)))
            .unwrap_or_else(|| "[binary]".to_string())),
        DataType::FixedSizeBinary(_) => json!(array
            .as_any()
            .downcast_ref::<FixedSizeBinaryArray>()
            .map(|array| blob_preview(array.value(index)))
            .unwrap_or_else(|| "[binary]".to_string())),
        DataType::Date32 => json!(array
            .as_any()
            .downcast_ref::<Date32Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Date64 => json!(array
            .as_any()
            .downcast_ref::<Date64Array>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Timestamp(TimeUnit::Second, _) => json!(array
            .as_any()
            .downcast_ref::<TimestampSecondArray>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Timestamp(TimeUnit::Millisecond, _) => json!(array
            .as_any()
            .downcast_ref::<TimestampMillisecondArray>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Timestamp(TimeUnit::Microsecond, _) => json!(array
            .as_any()
            .downcast_ref::<TimestampMicrosecondArray>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Timestamp(TimeUnit::Nanosecond, _) => json!(array
            .as_any()
            .downcast_ref::<TimestampNanosecondArray>()
            .map(|array| array.value(index))
            .unwrap_or_default()),
        DataType::Decimal128(_, _) => json!(array
            .as_any()
            .downcast_ref::<Decimal128Array>()
            .map(|array| array.value(index).to_string())
            .unwrap_or_default()),
        DataType::FixedSizeList(_, _) => array
            .as_any()
            .downcast_ref::<FixedSizeListArray>()
            .map(|array| list_preview(array.value(index).as_ref()))
            .unwrap_or_else(|| json!("[list]")),
        other => json!(format!("[{}]", data_type_label(other))),
    }
}

fn finite_f64_json(value: f64) -> Value {
    if value.is_finite() {
        json!(value)
    } else {
        json!(value.to_string())
    }
}

fn list_preview(array: &dyn Array) -> Value {
    json!(format!(
        "[{} {} value{}]",
        array.len(),
        data_type_label(array.data_type()),
        if array.len() == 1 { "" } else { "s" }
    ))
}

fn data_type_label(data_type: &DataType) -> String {
    match data_type {
        DataType::Utf8 => "TEXT".to_string(),
        DataType::LargeUtf8 => "LARGE_TEXT".to_string(),
        DataType::Boolean => "BOOLEAN".to_string(),
        DataType::Int8 => "INT8".to_string(),
        DataType::Int16 => "INT16".to_string(),
        DataType::Int32 => "INT32".to_string(),
        DataType::Int64 => "INT64".to_string(),
        DataType::UInt8 => "UINT8".to_string(),
        DataType::UInt16 => "UINT16".to_string(),
        DataType::UInt32 => "UINT32".to_string(),
        DataType::UInt64 => "UINT64".to_string(),
        DataType::Float32 => "FLOAT32".to_string(),
        DataType::Float64 => "FLOAT64".to_string(),
        DataType::Binary => "BINARY".to_string(),
        DataType::LargeBinary => "LARGE_BINARY".to_string(),
        DataType::FixedSizeBinary(size) => format!("BINARY[{size}]"),
        DataType::FixedSizeList(field, size) => {
            format!("{}[{size}]", data_type_label(field.data_type()))
        }
        DataType::List(field) => format!("LIST<{}>", data_type_label(field.data_type())),
        DataType::Date32 => "DATE32".to_string(),
        DataType::Date64 => "DATE64".to_string(),
        DataType::Timestamp(unit, _) => format!("TIMESTAMP_{unit:?}"),
        DataType::Decimal128(precision, scale) => format!("DECIMAL128({precision},{scale})"),
        other => format!("{other:?}"),
    }
}

fn is_sensitive_cell(table_name: &str, column_name: &str) -> bool {
    let table = table_name.to_ascii_lowercase();
    let column = column_name.to_ascii_lowercase();
    table.contains("credential")
        || table.contains("session")
        || column.contains("secret")
        || column.contains("token")
        || column.contains("password")
        || column.contains("credential")
        || column.contains("api_key")
        || column.contains("private")
        || column == "payload"
        || column.ends_with("_payload")
        || column.ends_with("_json") && table.contains("profile")
}

fn quote_identifier(identifier: &str) -> String {
    let escaped = identifier.replace('"', "\"\"");
    format!("\"{escaped}\"")
}

fn normalize_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(DEFAULT_ROW_LIMIT).clamp(1, MAX_ROW_LIMIT)
}

fn preview_text(value: &str) -> String {
    if value.len() <= VALUE_PREVIEW_LIMIT {
        return value.to_string();
    }

    let mut boundary = VALUE_PREVIEW_LIMIT;
    while !value.is_char_boundary(boundary) && boundary > 0 {
        boundary -= 1;
    }
    format!("{}... [{} bytes total]", &value[..boundary], value.len())
}

fn blob_preview(bytes: &[u8]) -> String {
    let visible_len = bytes.len().min(BLOB_PREVIEW_LIMIT);
    let encoded = BASE64_STANDARD.encode(&bytes[..visible_len]);
    if bytes.len() > visible_len {
        format!("base64:{encoded}... [{} bytes total]", bytes.len())
    } else {
        format!("base64:{encoded}")
    }
}

fn fallback_project_name(root_path: &str) -> String {
    Path::new(root_path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_string)
        .unwrap_or_else(|| root_path.to_string())
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

fn storage_error(code: &'static str, error: impl std::fmt::Display) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero could not inspect local storage: {error}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_identifier_escapes_embedded_quotes() {
        assert_eq!(quote_identifier("safe"), "\"safe\"");
        assert_eq!(quote_identifier("a\"b"), "\"a\"\"b\"");
    }

    #[test]
    fn redacts_sensitive_tables_and_columns_by_default() {
        assert!(is_sensitive_cell("provider_credentials", "provider_id"));
        assert!(is_sensitive_cell("projects", "oauth_access_token"));
        assert!(is_sensitive_cell("runtime_settings", "payload"));
        assert!(!is_sensitive_cell("projects", "name"));
    }

    #[test]
    fn normalizes_row_limit_to_bounded_window() {
        assert_eq!(normalize_limit(None), DEFAULT_ROW_LIMIT);
        assert_eq!(normalize_limit(Some(0)), 1);
        assert_eq!(normalize_limit(Some(5)), 5);
        assert_eq!(normalize_limit(Some(MAX_ROW_LIMIT + 1)), MAX_ROW_LIMIT);
    }

    #[test]
    fn formats_storage_display_values_without_frontend_json_stringify() {
        assert_eq!(storage_display_value(&Value::Null), "NULL");
        assert_eq!(storage_display_value(&json!("plain")), "plain");
        assert_eq!(storage_display_value(&json!({"ok": true})), "{\"ok\":true}");

        let mut values = Map::new();
        values.insert("payload".into(), json!({"nested": [1, 2]}));
        let row = developer_storage_row(values);
        assert_eq!(
            row.display_values.get("payload").map(String::as_str),
            Some("{\"nested\":[1,2]}")
        );
    }
}
