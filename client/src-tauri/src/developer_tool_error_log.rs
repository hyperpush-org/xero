use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use rand::RngCore;
use rusqlite::{params, params_from_iter, types::Value as SqlValue, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    runtime::redaction::redact_json_for_persistence,
};

const LAUNCH_MODE_ENV: &str = "XERO_LAUNCH_MODE";
const LOCAL_SOURCE_LAUNCH_MODE: &str = "local-source";
const DEVELOPMENT_DIRECTORY: &str = "development";
const DATABASE_FILE_NAME: &str = "tool-call-errors.sqlite";
const SCHEMA_VERSION: i64 = 1;
const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 500;
const MESSAGE_PREVIEW_LIMIT: usize = 220;

const TABLE_COLUMNS: &[(&str, &str)] = &[
    ("id", "TEXT"),
    ("occurred_at", "TEXT"),
    ("source", "TEXT"),
    ("project_id", "TEXT"),
    ("agent_session_id", "TEXT"),
    ("run_id", "TEXT"),
    ("turn_index", "INTEGER"),
    ("tool_call_id", "TEXT"),
    ("tool_name", "TEXT"),
    ("input_sha256", "TEXT"),
    ("input_json", "TEXT"),
    ("input_redacted", "INTEGER"),
    ("error_code", "TEXT"),
    ("error_class", "TEXT"),
    ("error_category", "TEXT"),
    ("error_message", "TEXT"),
    ("model_message", "TEXT"),
    ("retryable", "INTEGER"),
    ("dispatch_json", "TEXT"),
    ("context_json", "TEXT"),
];

const INDEX_NAMES: &[&str] = &[
    "idx_tool_call_error_log_occurred_at",
    "idx_tool_call_error_log_tool_name",
    "idx_tool_call_error_log_error_code",
    "idx_tool_call_error_log_project",
];

const REQUIRED_TABLE_SQL_FRAGMENTS: &[&str] = &[
    "id text primary key check (id <> '')",
    "input_sha256 text not null check (length(input_sha256) = 64)",
    "input_json text not null check (input_json <> '' and json_valid(input_json))",
    "input_redacted integer not null check (input_redacted in (0, 1))",
    "retryable integer not null check (retryable in (0, 1))",
    "dispatch_json text not null check (dispatch_json <> '' and json_valid(dispatch_json))",
    "context_json text not null check (context_json <> '' and json_valid(context_json))",
    ") strict",
];

#[derive(Debug, Clone)]
pub(crate) struct ToolCallErrorLogEntryDraft {
    pub source: String,
    pub project_id: Option<String>,
    pub agent_session_id: Option<String>,
    pub run_id: Option<String>,
    pub turn_index: Option<i64>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input: JsonValue,
    pub error_code: String,
    pub error_class: String,
    pub error_category: Option<String>,
    pub error_message: String,
    pub model_message: Option<String>,
    pub retryable: bool,
    pub dispatch: JsonValue,
    pub context: JsonValue,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolErrorLogEntryDto {
    pub id: String,
    pub occurred_at: String,
    pub source: String,
    pub project_id: Option<String>,
    pub agent_session_id: Option<String>,
    pub run_id: Option<String>,
    pub turn_index: Option<i64>,
    pub tool_call_id: String,
    pub tool_name: String,
    pub input_sha256: String,
    pub input_json: JsonValue,
    pub input_redacted: bool,
    pub error_code: String,
    pub error_class: String,
    pub error_category: Option<String>,
    pub error_message: String,
    pub model_message: Option<String>,
    pub retryable: bool,
    pub dispatch_json: JsonValue,
    pub context_json: JsonValue,
    pub message_preview: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolErrorLogListRequestDto {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
    pub project_id: Option<String>,
    pub tool_name: Option<String>,
    pub error_code: Option<String>,
    pub query: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolErrorLogListResponseDto {
    pub database_path: String,
    pub entries: Vec<DeveloperToolErrorLogEntryDto>,
    pub project_ids: Vec<String>,
    pub total_count: u64,
    pub limit: u32,
    pub offset: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeveloperToolErrorLogClearResponseDto {
    pub database_path: String,
    pub cleared_count: u64,
}

pub(crate) fn dev_tool_error_log_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir
        .join(DEVELOPMENT_DIRECTORY)
        .join(DATABASE_FILE_NAME)
}

pub(crate) fn developer_tool_error_log_enabled() -> bool {
    cfg!(debug_assertions) && launch_mode_is_local_source()
}

pub(crate) fn ensure_developer_tool_error_log_available() -> CommandResult<()> {
    if developer_tool_error_log_enabled() {
        return Ok(());
    }

    Err(CommandError::user_fixable(
        "developer_tool_error_log_unavailable",
        "Developer tool-call error logging is only available in debug builds launched with XERO_LAUNCH_MODE=local-source.",
    ))
}

pub(crate) fn log_tool_call_failure_best_effort(
    app_data_dir: Option<&Path>,
    draft: ToolCallErrorLogEntryDraft,
) {
    #[cfg(debug_assertions)]
    {
        if !launch_mode_is_local_source() {
            return;
        }
        let Some(app_data_dir) = app_data_dir else {
            return;
        };
        if let Err(error) = insert_tool_call_error_log_entry(app_data_dir, draft) {
            eprintln!("[developer-tool-error-log] write skipped: {error}");
        }
    }

    #[cfg(not(debug_assertions))]
    {
        let _ = app_data_dir;
        let _ = draft;
    }
}

pub(crate) fn insert_tool_call_error_log_entry(
    app_data_dir: &Path,
    draft: ToolCallErrorLogEntryDraft,
) -> CommandResult<()> {
    validate_draft(&draft)?;
    let (connection, _database_path) = open_tool_error_log_database(app_data_dir)?;
    insert_tool_call_error_log_entry_with_connection(&connection, draft)
}

pub(crate) fn list_tool_call_error_log_entries(
    app_data_dir: &Path,
    request: DeveloperToolErrorLogListRequestDto,
) -> CommandResult<DeveloperToolErrorLogListResponseDto> {
    let limit = normalize_limit(request.limit);
    let offset = request.offset.unwrap_or(0);
    let (connection, database_path) = open_tool_error_log_database(app_data_dir)?;
    let project_ids = list_project_ids(&connection)?;
    let filters = build_filters(&request);
    let where_sql = if filters.conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", filters.conditions.join(" AND "))
    };

    let mut count_statement = connection
        .prepare(&format!(
            "SELECT COUNT(*) FROM tool_call_error_log{where_sql}"
        ))
        .map_err(|error| database_error("developer_tool_error_log_query_failed", error))?;
    let total_count: u64 = count_statement
        .query_row(params_from_iter(filters.params.iter()), |row| {
            row.get::<_, i64>(0).map(|count| count.max(0) as u64)
        })
        .map_err(|error| database_error("developer_tool_error_log_query_failed", error))?;

    let mut query_params = filters.params;
    query_params.push(SqlValue::Integer(i64::from(limit)));
    query_params.push(SqlValue::Integer(i64::from(offset)));
    let mut statement = connection
        .prepare(&format!(
            "SELECT \
                id, occurred_at, source, project_id, agent_session_id, run_id, turn_index, \
                tool_call_id, tool_name, input_sha256, input_json, input_redacted, \
                error_code, error_class, error_category, error_message, model_message, retryable, \
                dispatch_json, context_json \
             FROM tool_call_error_log{where_sql} \
             ORDER BY occurred_at DESC, id DESC \
             LIMIT ? OFFSET ?"
        ))
        .map_err(|error| database_error("developer_tool_error_log_query_failed", error))?;
    let rows = statement
        .query_map(params_from_iter(query_params.iter()), map_entry_row)
        .map_err(|error| database_error("developer_tool_error_log_query_failed", error))?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(
            row.map_err(|error| database_error("developer_tool_error_log_query_failed", error))?,
        );
    }

    Ok(DeveloperToolErrorLogListResponseDto {
        database_path: display_path(&database_path),
        entries,
        project_ids,
        total_count,
        limit,
        offset,
    })
}

pub(crate) fn clear_tool_call_error_log(
    app_data_dir: &Path,
) -> CommandResult<DeveloperToolErrorLogClearResponseDto> {
    let (connection, database_path) = open_tool_error_log_database(app_data_dir)?;
    let cleared_count: u64 = connection
        .query_row("SELECT COUNT(*) FROM tool_call_error_log", [], |row| {
            row.get::<_, i64>(0).map(|count| count.max(0) as u64)
        })
        .map_err(|error| database_error("developer_tool_error_log_clear_failed", error))?;
    connection
        .execute("DELETE FROM tool_call_error_log", [])
        .map_err(|error| database_error("developer_tool_error_log_clear_failed", error))?;

    Ok(DeveloperToolErrorLogClearResponseDto {
        database_path: display_path(&database_path),
        cleared_count,
    })
}

fn open_tool_error_log_database(app_data_dir: &Path) -> CommandResult<(Connection, PathBuf)> {
    let database_path = dev_tool_error_log_path(app_data_dir);
    let database_existed = database_path.exists();
    let mut connection = open_configured_connection(&database_path)?;

    if database_existed && !schema_is_current(&connection)? {
        close_connection(connection)?;
        delete_database_and_sidecars(&database_path)?;
        connection = open_configured_connection(&database_path)?;
    }

    apply_schema(&connection)?;
    if !schema_is_current(&connection)? {
        return Err(CommandError::system_fault(
            "developer_tool_error_log_schema_invalid",
            format!(
                "Xero could not create the developer tool-call error log schema at {}.",
                database_path.display()
            ),
        ));
    }

    Ok((connection, database_path))
}

fn open_configured_connection(database_path: &Path) -> CommandResult<Connection> {
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "developer_tool_error_log_dir_unavailable",
                format!(
                    "Xero could not prepare the developer tool-call error log directory at {}: {error}",
                    parent.display()
                ),
            )
        })?;
    }

    let connection = Connection::open(database_path)
        .map_err(|error| database_error("developer_tool_error_log_open_failed", error))?;
    connection
        .busy_timeout(Duration::from_secs(5))
        .map_err(|error| database_error("developer_tool_error_log_open_failed", error))?;
    connection
        .execute_batch("PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")
        .map_err(|error| database_error("developer_tool_error_log_open_failed", error))?;
    connection
        .query_row("PRAGMA journal_mode = WAL", [], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| database_error("developer_tool_error_log_open_failed", error))?;
    connection
        .execute_batch("PRAGMA wal_autocheckpoint = 1000;")
        .map_err(|error| database_error("developer_tool_error_log_open_failed", error))?;
    Ok(connection)
}

fn apply_schema(connection: &Connection) -> CommandResult<()> {
    connection
        .execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS tool_call_error_log (
                id TEXT PRIMARY KEY CHECK (id <> ''),
                occurred_at TEXT NOT NULL CHECK (occurred_at <> ''),
                source TEXT NOT NULL CHECK (source <> ''),
                project_id TEXT,
                agent_session_id TEXT,
                run_id TEXT,
                turn_index INTEGER,
                tool_call_id TEXT NOT NULL CHECK (tool_call_id <> ''),
                tool_name TEXT NOT NULL CHECK (tool_name <> ''),
                input_sha256 TEXT NOT NULL CHECK (length(input_sha256) = 64),
                input_json TEXT NOT NULL CHECK (input_json <> '' AND json_valid(input_json)),
                input_redacted INTEGER NOT NULL CHECK (input_redacted IN (0, 1)),
                error_code TEXT NOT NULL CHECK (error_code <> ''),
                error_class TEXT NOT NULL CHECK (error_class <> ''),
                error_category TEXT,
                error_message TEXT NOT NULL CHECK (error_message <> ''),
                model_message TEXT,
                retryable INTEGER NOT NULL CHECK (retryable IN (0, 1)),
                dispatch_json TEXT NOT NULL CHECK (dispatch_json <> '' AND json_valid(dispatch_json)),
                context_json TEXT NOT NULL CHECK (context_json <> '' AND json_valid(context_json))
            ) STRICT;

            CREATE INDEX IF NOT EXISTS idx_tool_call_error_log_occurred_at
                ON tool_call_error_log(occurred_at DESC);
            CREATE INDEX IF NOT EXISTS idx_tool_call_error_log_tool_name
                ON tool_call_error_log(tool_name, occurred_at DESC);
            CREATE INDEX IF NOT EXISTS idx_tool_call_error_log_error_code
                ON tool_call_error_log(error_code, occurred_at DESC);
            CREATE INDEX IF NOT EXISTS idx_tool_call_error_log_project
                ON tool_call_error_log(project_id, occurred_at DESC);

            PRAGMA user_version = 1;
            "#,
        )
        .map_err(|error| database_error("developer_tool_error_log_schema_failed", error))
}

fn schema_is_current(connection: &Connection) -> CommandResult<bool> {
    let user_version: i64 = connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| database_error("developer_tool_error_log_schema_check_failed", error))?;
    if user_version != SCHEMA_VERSION {
        return Ok(false);
    }

    let strict = connection
        .query_row("PRAGMA table_list('tool_call_error_log')", [], |row| {
            row.get::<_, i64>(5)
        })
        .optional()
        .map_err(|error| database_error("developer_tool_error_log_schema_check_failed", error))?;
    if strict != Some(1) {
        return Ok(false);
    }

    let table_sql = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'tool_call_error_log'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| database_error("developer_tool_error_log_schema_check_failed", error))?;
    let Some(table_sql) = table_sql else {
        return Ok(false);
    };
    let normalized_table_sql = normalize_schema_sql(&table_sql);
    if REQUIRED_TABLE_SQL_FRAGMENTS
        .iter()
        .any(|fragment| !normalized_table_sql.contains(fragment))
    {
        return Ok(false);
    }

    let mut columns = Vec::new();
    let mut column_statement = connection
        .prepare("PRAGMA table_xinfo('tool_call_error_log')")
        .map_err(|error| database_error("developer_tool_error_log_schema_check_failed", error))?;
    let column_rows = column_statement
        .query_map([], |row| {
            Ok((row.get::<_, String>(1)?, row.get::<_, String>(2)?))
        })
        .map_err(|error| database_error("developer_tool_error_log_schema_check_failed", error))?;
    for row in column_rows {
        columns.push(row.map_err(|error| {
            database_error("developer_tool_error_log_schema_check_failed", error)
        })?);
    }
    if columns.len() != TABLE_COLUMNS.len()
        || columns.iter().zip(TABLE_COLUMNS.iter()).any(
            |((name, type_label), (expected_name, expected_type))| {
                name != expected_name || type_label.to_ascii_uppercase() != *expected_type
            },
        )
    {
        return Ok(false);
    }

    let mut index_statement = connection
        .prepare(
            "SELECT name FROM sqlite_schema \
             WHERE type = 'index' AND tbl_name = 'tool_call_error_log'",
        )
        .map_err(|error| database_error("developer_tool_error_log_schema_check_failed", error))?;
    let index_rows = index_statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| database_error("developer_tool_error_log_schema_check_failed", error))?;
    let mut index_names = Vec::new();
    for row in index_rows {
        index_names.push(row.map_err(|error| {
            database_error("developer_tool_error_log_schema_check_failed", error)
        })?);
    }
    Ok(INDEX_NAMES
        .iter()
        .all(|expected| index_names.iter().any(|name| name == expected)))
}

fn insert_tool_call_error_log_entry_with_connection(
    connection: &Connection,
    draft: ToolCallErrorLogEntryDraft,
) -> CommandResult<()> {
    let (input_json, input_redacted) = redact_json_for_persistence(&draft.input);
    let input_sha256 = sha256_json(&draft.input)?;
    let input_json = serialize_json(
        &input_json,
        "developer_tool_error_log_input_serialize_failed",
    )?;
    let dispatch_json = serialize_json(
        &draft.dispatch,
        "developer_tool_error_log_dispatch_serialize_failed",
    )?;
    let context_json = serialize_json(
        &draft.context,
        "developer_tool_error_log_context_serialize_failed",
    )?;
    let occurred_at = now_timestamp();

    connection
        .execute(
            r#"
            INSERT INTO tool_call_error_log (
                id,
                occurred_at,
                source,
                project_id,
                agent_session_id,
                run_id,
                turn_index,
                tool_call_id,
                tool_name,
                input_sha256,
                input_json,
                input_redacted,
                error_code,
                error_class,
                error_category,
                error_message,
                model_message,
                retryable,
                dispatch_json,
                context_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
            "#,
            params![
                generate_log_id(),
                occurred_at,
                draft.source,
                draft.project_id,
                draft.agent_session_id,
                draft.run_id,
                draft.turn_index,
                draft.tool_call_id,
                draft.tool_name,
                input_sha256,
                input_json,
                input_redacted,
                draft.error_code,
                draft.error_class,
                draft.error_category,
                draft.error_message,
                draft.model_message,
                draft.retryable,
                dispatch_json,
                context_json,
            ],
        )
        .map_err(|error| database_error("developer_tool_error_log_insert_failed", error))?;
    Ok(())
}

fn list_project_ids(connection: &Connection) -> CommandResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            "SELECT DISTINCT project_id \
             FROM tool_call_error_log \
             WHERE project_id IS NOT NULL AND trim(project_id) <> '' \
             ORDER BY project_id COLLATE NOCASE ASC",
        )
        .map_err(|error| database_error("developer_tool_error_log_query_failed", error))?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|error| database_error("developer_tool_error_log_query_failed", error))?;

    let mut project_ids = Vec::new();
    for row in rows {
        project_ids.push(
            row.map_err(|error| database_error("developer_tool_error_log_query_failed", error))?,
        );
    }
    Ok(project_ids)
}

#[derive(Debug, Default)]
struct QueryFilters {
    conditions: Vec<&'static str>,
    params: Vec<SqlValue>,
}

fn build_filters(request: &DeveloperToolErrorLogListRequestDto) -> QueryFilters {
    let mut filters = QueryFilters::default();
    push_optional_exact_filter(
        &mut filters,
        "project_id = ?",
        request.project_id.as_deref(),
    );
    push_optional_exact_filter(&mut filters, "tool_name = ?", request.tool_name.as_deref());
    push_optional_exact_filter(
        &mut filters,
        "error_code = ?",
        request.error_code.as_deref(),
    );

    if let Some(query) = normalize_optional_text(request.query.as_deref()) {
        let pattern = format!("%{}%", escape_like_pattern(query));
        filters.conditions.push(
            "(tool_name LIKE ? ESCAPE '\\' \
              OR error_code LIKE ? ESCAPE '\\' \
              OR error_category LIKE ? ESCAPE '\\' \
              OR error_message LIKE ? ESCAPE '\\' \
              OR tool_call_id LIKE ? ESCAPE '\\' \
              OR run_id LIKE ? ESCAPE '\\')",
        );
        for _ in 0..6 {
            filters.params.push(SqlValue::Text(pattern.clone()));
        }
    }

    filters
}

fn push_optional_exact_filter(
    filters: &mut QueryFilters,
    condition: &'static str,
    value: Option<&str>,
) {
    if let Some(value) = normalize_optional_text(value) {
        filters.conditions.push(condition);
        filters.params.push(SqlValue::Text(value.to_owned()));
    }
}

fn map_entry_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<DeveloperToolErrorLogEntryDto> {
    let error_message: String = row.get(15)?;
    Ok(DeveloperToolErrorLogEntryDto {
        id: row.get(0)?,
        occurred_at: row.get(1)?,
        source: row.get(2)?,
        project_id: row.get(3)?,
        agent_session_id: row.get(4)?,
        run_id: row.get(5)?,
        turn_index: row.get(6)?,
        tool_call_id: row.get(7)?,
        tool_name: row.get(8)?,
        input_sha256: row.get(9)?,
        input_json: parse_stored_json(row.get::<_, String>(10)?),
        input_redacted: row.get(11)?,
        error_code: row.get(12)?,
        error_class: row.get(13)?,
        error_category: row.get(14)?,
        message_preview: message_preview(&error_message),
        error_message,
        model_message: row.get(16)?,
        retryable: row.get(17)?,
        dispatch_json: parse_stored_json(row.get::<_, String>(18)?),
        context_json: parse_stored_json(row.get::<_, String>(19)?),
    })
}

fn parse_stored_json(raw: String) -> JsonValue {
    serde_json::from_str(&raw).unwrap_or(JsonValue::Null)
}

fn validate_draft(draft: &ToolCallErrorLogEntryDraft) -> CommandResult<()> {
    validate_text(&draft.source, "source")?;
    validate_text(&draft.tool_call_id, "toolCallId")?;
    validate_text(&draft.tool_name, "toolName")?;
    validate_text(&draft.error_code, "errorCode")?;
    validate_text(&draft.error_class, "errorClass")?;
    validate_text(&draft.error_message, "errorMessage")?;
    Ok(())
}

fn validate_text(value: &str, field: &'static str) -> CommandResult<()> {
    if value.trim().is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(())
}

fn normalize_limit(limit: Option<u32>) -> u32 {
    limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT)
}

fn normalize_optional_text(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

fn normalize_schema_sql(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn escape_like_pattern(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

fn message_preview(message: &str) -> String {
    let trimmed = message.trim();
    let mut out = String::new();
    for character in trimmed.chars().take(MESSAGE_PREVIEW_LIMIT) {
        out.push(character);
    }
    if trimmed.chars().count() > MESSAGE_PREVIEW_LIMIT {
        out.push_str("...");
    }
    out
}

fn sha256_json(value: &JsonValue) -> CommandResult<String> {
    let bytes = serde_json::to_vec(value).map_err(|error| {
        CommandError::system_fault(
            "developer_tool_error_log_input_hash_failed",
            format!("Xero could not hash developer tool-call input: {error}"),
        )
    })?;
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

fn serialize_json(value: &JsonValue, code: &'static str) -> CommandResult<String> {
    serde_json::to_string(value).map_err(|error| {
        CommandError::system_fault(
            code,
            format!("Xero could not serialize developer tool-call error log JSON: {error}"),
        )
    })
}

fn generate_log_id() -> String {
    let mut bytes = [0_u8; 12];
    rand::thread_rng().fill_bytes(&mut bytes);
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("tool-error-{hex}")
}

fn launch_mode_is_local_source() -> bool {
    env::var(LAUNCH_MODE_ENV)
        .ok()
        .as_deref()
        .is_some_and(|value| value == LOCAL_SOURCE_LAUNCH_MODE)
}

fn close_connection(connection: Connection) -> CommandResult<()> {
    connection.close().map_err(|(_connection, error)| {
        database_error("developer_tool_error_log_close_failed", error)
    })
}

fn delete_database_and_sidecars(database_path: &Path) -> CommandResult<()> {
    for path in [
        database_path.to_path_buf(),
        sqlite_sidecar_path(database_path, "-wal"),
        sqlite_sidecar_path(database_path, "-shm"),
    ] {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(CommandError::retryable(
                    "developer_tool_error_log_reset_failed",
                    format!(
                        "Xero could not remove stale developer tool-call error log file {}: {error}",
                        path.display()
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn sqlite_sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    let file_name = database_path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or(DATABASE_FILE_NAME);
    database_path.with_file_name(format!("{file_name}{suffix}"))
}

fn database_error(code: &'static str, error: rusqlite::Error) -> CommandError {
    CommandError::retryable(
        code,
        format!("Xero could not access the developer tool-call error log: {error}"),
    )
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    use serde_json::json;

    fn draft(
        tool_name: &str,
        error_code: &str,
        occurred_marker: &str,
    ) -> ToolCallErrorLogEntryDraft {
        ToolCallErrorLogEntryDraft {
            source: "tool_registry_v2_dispatch".into(),
            project_id: Some("project-1".into()),
            agent_session_id: Some("session-1".into()),
            run_id: Some(format!("run-{occurred_marker}")),
            turn_index: Some(2),
            tool_call_id: format!("call-{occurred_marker}"),
            tool_name: tool_name.into(),
            input: json!({
                "path": "src/main.rs",
                "api_key": "sk-live-secret",
                "safe": occurred_marker,
            }),
            error_code: error_code.into(),
            error_class: "retryable".into(),
            error_category: Some("retryable_provider_tool_failure".into()),
            error_message: format!("Failure {occurred_marker}"),
            model_message: Some("Retry with new context.".into()),
            retryable: true,
            dispatch: json!({ "groupMode": "parallel_read_only", "marker": occurred_marker }),
            context: json!({ "launchMode": "local-source", "marker": occurred_marker }),
        }
    }

    #[test]
    fn developer_tool_error_log_initializes_v1_schema_wal_and_indexes() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let (connection, database_path) =
            open_tool_error_log_database(tempdir.path()).expect("open dev log");

        assert_eq!(
            database_path,
            tempdir
                .path()
                .join(DEVELOPMENT_DIRECTORY)
                .join(DATABASE_FILE_NAME)
        );
        let foreign_keys: i64 = connection
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .expect("foreign keys");
        let journal_mode: String = connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal mode");
        let synchronous: i64 = connection
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .expect("synchronous");
        let user_version: i64 = connection
            .query_row("PRAGMA user_version", [], |row| row.get(0))
            .expect("user version");

        assert_eq!(foreign_keys, 1);
        assert_eq!(journal_mode.to_ascii_lowercase(), "wal");
        assert_eq!(synchronous, 1);
        assert_eq!(user_version, SCHEMA_VERSION);
        assert!(schema_is_current(&connection).expect("schema current"));
    }

    #[test]
    fn stale_schema_version_wipes_and_recreates_dev_database() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let database_path = dev_tool_error_log_path(tempdir.path());
        fs::create_dir_all(database_path.parent().expect("database parent"))
            .expect("create parent");
        {
            let connection = Connection::open(&database_path).expect("seed stale db");
            connection
                .execute_batch("CREATE TABLE stale(value TEXT); PRAGMA user_version = 99;")
                .expect("seed stale schema");
        }

        let (connection, _) = open_tool_error_log_database(tempdir.path()).expect("reopen dev log");
        let stale_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'stale'",
                [],
                |row| row.get(0),
            )
            .expect("stale count");

        assert_eq!(stale_count, 0);
        assert!(schema_is_current(&connection).expect("schema current"));
    }

    #[test]
    fn stale_constraint_schema_wipes_and_recreates_dev_database() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let database_path = dev_tool_error_log_path(tempdir.path());
        fs::create_dir_all(database_path.parent().expect("database parent"))
            .expect("create parent");
        {
            let connection = Connection::open(&database_path).expect("seed stale db");
            connection
                .execute_batch(
                    r#"
                    CREATE TABLE tool_call_error_log (
                        id TEXT PRIMARY KEY,
                        occurred_at TEXT NOT NULL,
                        source TEXT NOT NULL,
                        project_id TEXT,
                        agent_session_id TEXT,
                        run_id TEXT,
                        turn_index INTEGER,
                        tool_call_id TEXT NOT NULL,
                        tool_name TEXT NOT NULL,
                        input_sha256 TEXT NOT NULL,
                        input_json TEXT NOT NULL,
                        input_redacted INTEGER NOT NULL,
                        error_code TEXT NOT NULL,
                        error_class TEXT NOT NULL,
                        error_category TEXT,
                        error_message TEXT NOT NULL,
                        model_message TEXT,
                        retryable INTEGER NOT NULL,
                        dispatch_json TEXT NOT NULL,
                        context_json TEXT NOT NULL
                    ) STRICT;
                    CREATE TABLE stale_marker(value TEXT);
                    CREATE INDEX idx_tool_call_error_log_occurred_at
                        ON tool_call_error_log(occurred_at DESC);
                    CREATE INDEX idx_tool_call_error_log_tool_name
                        ON tool_call_error_log(tool_name, occurred_at DESC);
                    CREATE INDEX idx_tool_call_error_log_error_code
                        ON tool_call_error_log(error_code, occurred_at DESC);
                    CREATE INDEX idx_tool_call_error_log_project
                        ON tool_call_error_log(project_id, occurred_at DESC);
                    PRAGMA user_version = 1;
                    "#,
                )
                .expect("seed stale constraints");
        }

        let (connection, _) = open_tool_error_log_database(tempdir.path()).expect("reopen dev log");
        let stale_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM sqlite_schema WHERE type = 'table' AND name = 'stale_marker'",
                [],
                |row| row.get(0),
            )
            .expect("stale marker count");

        assert_eq!(stale_count, 0);
        assert!(schema_is_current(&connection).expect("schema current"));
    }

    #[test]
    fn insert_redacts_input_and_stores_original_hash() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let raw_input = draft("command", "command_failed", "one").input;
        let expected_hash = sha256_json(&raw_input).expect("input hash");
        let mut entry = draft("command", "command_failed", "one");
        entry.input = raw_input;

        insert_tool_call_error_log_entry(tempdir.path(), entry).expect("insert");
        let response = list_tool_call_error_log_entries(
            tempdir.path(),
            DeveloperToolErrorLogListRequestDto {
                limit: None,
                offset: None,
                project_id: None,
                tool_name: None,
                error_code: None,
                query: None,
            },
        )
        .expect("list");
        let logged = response.entries.first().expect("entry");

        assert_eq!(logged.input_sha256, expected_hash);
        assert_eq!(logged.input_json["api_key"], json!("[REDACTED]"));
        assert!(logged.input_redacted);
        assert!(!logged.input_json.to_string().contains("sk-live-secret"));
    }

    #[test]
    fn query_filters_are_parameterized_and_return_newest_first() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let mut old = draft("read", "read_failed", "old");
        old.project_id = Some("project-2".into());
        insert_tool_call_error_log_entry(tempdir.path(), old).expect("insert old");
        insert_tool_call_error_log_entry(tempdir.path(), draft("write", "write_failed", "new"))
            .expect("insert new");

        let response = list_tool_call_error_log_entries(
            tempdir.path(),
            DeveloperToolErrorLogListRequestDto {
                limit: Some(10),
                offset: Some(0),
                project_id: Some("project-1".into()),
                tool_name: None,
                error_code: None,
                query: Some("write".into()),
            },
        )
        .expect("filtered list");

        assert_eq!(response.total_count, 1);
        assert_eq!(response.project_ids, vec!["project-1", "project-2"]);
        assert_eq!(response.entries[0].tool_name, "write");
        assert_eq!(response.entries[0].error_code, "write_failed");

        let escaped = list_tool_call_error_log_entries(
            tempdir.path(),
            DeveloperToolErrorLogListRequestDto {
                limit: Some(10),
                offset: Some(0),
                project_id: None,
                tool_name: None,
                error_code: None,
                query: Some("%".into()),
            },
        )
        .expect("escaped list");
        assert_eq!(escaped.total_count, 0);
    }

    #[test]
    fn best_effort_logging_does_not_surface_write_failures() {
        let tempdir = tempfile::tempdir().expect("temp dir");
        let app_data_file = tempdir.path().join("not-a-directory");
        fs::write(&app_data_file, "file").expect("seed file");

        log_tool_call_failure_best_effort(Some(&app_data_file), draft("read", "failed", "one"));
    }
}
