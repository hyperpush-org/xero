//! SQLite → Lance one-shot migration for `agent_memories`.
//!
//! Runs as part of the per-project schema migration (see
//! `db/migrations.rs`). The hook drains rows from the legacy SQLite
//! `agent_memories` table into a JSON staging file colocated with the
//! database, then drops the SQLite table and its indexes/triggers in the same
//! transaction. The staging file is consumed by [`drain_pending_into_lance`]
//! the next time the project DB is opened, which writes the rows into the
//! per-project LanceDB dataset and removes the staging payload.
//!
//! The split is deliberate: rusqlite_migration hooks are sync, but lancedb is
//! async and must run on a tokio runtime that is unsafe to nest under SQLite's
//! migration transaction. Staging in JSON keeps the migration deterministic
//! and recoverable — if the lance import fails, the staging file remains and
//! the next open retries.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{OptionalExtension, Transaction};
use rusqlite_migration::{HookError, HookResult};
use serde::{Deserialize, Serialize};

use crate::commands::CommandError;

use super::agent_memory_lance::{
    self, pending_import_path, AgentMemoryRow, AGENT_MEMORIES_DATASET_NAME,
};
use super::agent_memory::{AgentMemoryKind, AgentMemoryReviewState, AgentMemoryScope};

const DROP_LEGACY_AGENT_MEMORIES_SQL: &str = r#"
DROP TRIGGER IF EXISTS agent_memories_clear_deleted_session_sources;
DROP TRIGGER IF EXISTS agent_memories_clear_deleted_source_run;
DROP INDEX IF EXISTS idx_agent_memories_active_text;
DROP INDEX IF EXISTS idx_agent_memories_source_run;
DROP INDEX IF EXISTS idx_agent_memories_scope_review;
DROP TABLE IF EXISTS agent_memories;
"#;

/// Wire format for the staging JSON file. Stable enough to survive an app
/// crash mid-migration: a future Cadence build only needs to recognize this
/// shape long enough to drain it into Lance, then the file is removed.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingImport {
    pub schema_version: u32,
    pub project_id: Option<String>,
    pub rows: Vec<PendingRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRow {
    pub memory_id: String,
    pub agent_session_id: Option<String>,
    pub scope: String,
    pub kind: String,
    pub text: String,
    pub text_hash: String,
    pub review_state: String,
    pub enabled: bool,
    pub confidence: Option<u8>,
    pub source_run_id: Option<String>,
    pub source_item_ids: Vec<String>,
    pub diagnostic: Option<PendingDiagnostic>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingDiagnostic {
    pub code: String,
    pub message: String,
}

const PENDING_IMPORT_SCHEMA_VERSION: u32 = 1;

pub fn migrate_agent_memories_to_lance_pending(transaction: &Transaction<'_>) -> HookResult {
    let database_path: Option<PathBuf> = transaction
        .path()
        .filter(|path| !path.is_empty())
        .map(PathBuf::from);

    let project_id = read_project_id(transaction)?;

    let rows = if has_agent_memories_table(transaction)? {
        read_legacy_rows(transaction)?
    } else {
        Vec::new()
    };

    if let Some(path) = database_path.as_deref() {
        if !rows.is_empty() {
            write_pending_import(path, &project_id, &rows).map_err(|error| {
                HookError::Hook(format!(
                    "agent_memories lance staging write failed at {}: {error}",
                    path.display()
                ))
            })?;
        }
    }
    // If the path is unavailable (e.g. an in-memory database used by tests),
    // we still drop the table — there cannot be persisted rows to lose, and
    // running tests should not block on Lance imports.

    transaction.execute_batch(DROP_LEGACY_AGENT_MEMORIES_SQL)?;
    Ok(())
}

fn has_agent_memories_table(transaction: &Transaction<'_>) -> Result<bool, rusqlite::Error> {
    let count: i64 = transaction.query_row(
        "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'agent_memories'",
        [],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}

fn read_project_id(transaction: &Transaction<'_>) -> Result<Option<String>, rusqlite::Error> {
    let has_meta: i64 = transaction.query_row(
        "SELECT COUNT(1) FROM sqlite_master WHERE type = 'table' AND name = 'meta'",
        [],
        |row| row.get(0),
    )?;
    if has_meta == 0 {
        return Ok(None);
    }
    let project_id: Option<String> = transaction
        .query_row(
            "SELECT project_id FROM meta WHERE id = 1",
            [],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()?
        .flatten();
    Ok(project_id)
}

fn read_legacy_rows(transaction: &Transaction<'_>) -> Result<Vec<PendingRow>, rusqlite::Error> {
    let mut statement = transaction.prepare(
        r#"
        SELECT
            memory_id,
            agent_session_id,
            scope_kind,
            memory_kind,
            text,
            text_hash,
            review_state,
            enabled,
            confidence,
            source_run_id,
            source_item_ids_json,
            diagnostic_json,
            created_at,
            updated_at
        FROM agent_memories
        ORDER BY created_at ASC, memory_id ASC
        "#,
    )?;
    let rows = statement.query_map([], |row| {
        let source_item_ids_json: String = row.get(10)?;
        let source_item_ids: Vec<String> = serde_json::from_str(&source_item_ids_json)
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    10,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        let diagnostic_json: Option<String> = row.get(11)?;
        let diagnostic = diagnostic_json
            .as_deref()
            .map(parse_diagnostic_value)
            .transpose()
            .map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    11,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?;
        Ok(PendingRow {
            memory_id: row.get(0)?,
            agent_session_id: row.get(1)?,
            scope: row.get(2)?,
            kind: row.get(3)?,
            text: row.get(4)?,
            text_hash: row.get(5)?,
            review_state: row.get(6)?,
            enabled: row.get::<_, i64>(7)? == 1,
            confidence: row.get(8)?,
            source_run_id: row.get(9)?,
            source_item_ids,
            diagnostic,
            created_at: row.get(12)?,
            updated_at: row.get(13)?,
        })
    })?;
    rows.collect::<Result<Vec<_>, _>>()
}

fn parse_diagnostic_value(value: &str) -> Result<PendingDiagnostic, serde_json::Error> {
    let parsed: serde_json::Value = serde_json::from_str(value)?;
    Ok(PendingDiagnostic {
        code: parsed
            .get("code")
            .and_then(|value| value.as_str())
            .unwrap_or("agent_memory_diagnostic")
            .to_string(),
        message: parsed
            .get("message")
            .and_then(|value| value.as_str())
            .unwrap_or("Cadence could not decode memory diagnostic details.")
            .to_string(),
    })
}

fn write_pending_import(
    database_path: &Path,
    project_id: &Option<String>,
    rows: &[PendingRow],
) -> Result<(), std::io::Error> {
    let target = pending_import_path(database_path);
    let payload = PendingImport {
        schema_version: PENDING_IMPORT_SCHEMA_VERSION,
        project_id: project_id.clone(),
        rows: rows.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&payload).map_err(std::io::Error::other)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&target, json)?;
    Ok(())
}

/// Drain any staged lance-import payload sitting next to `database_path` into
/// the per-project Lance dataset. Returns Ok(true) if a drain happened.
///
/// Errors are reported through `CommandError`; a failed drain leaves the
/// pending file in place so the next open will retry. Callers should log but
/// not fail the database open if this returns Err — the project is still
/// usable, the lance store will just keep retrying on each access.
pub fn drain_pending_into_lance(
    database_path: &Path,
    fallback_project_id: &str,
) -> Result<bool, CommandError> {
    let pending_path = pending_import_path(database_path);
    if !pending_path.exists() {
        return Ok(false);
    }

    let payload = read_pending_payload(&pending_path)?;
    let project_id = payload
        .project_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| fallback_project_id.to_string());

    let store = agent_memory_lance::open_for_database_path(database_path, &project_id);
    let rows = payload
        .rows
        .into_iter()
        .map(|row| pending_row_to_lance_row(row, &project_id))
        .collect::<Vec<_>>();

    let written = store.drain_pending_import(rows)?;
    let _ = AGENT_MEMORIES_DATASET_NAME; // keep export referenced

    fs::remove_file(&pending_path).map_err(|error| {
        CommandError::retryable(
            "agent_memory_lance_pending_remove_failed",
            format!(
                "Cadence drained {written} memories into Lance but could not remove the staging file at {}: {error}",
                pending_path.display()
            ),
        )
    })?;
    Ok(true)
}

fn read_pending_payload(path: &Path) -> Result<PendingImport, CommandError> {
    let bytes = fs::read(path).map_err(|error| {
        CommandError::retryable(
            "agent_memory_lance_pending_read_failed",
            format!(
                "Cadence could not read the staged lance import at {}: {error}",
                path.display()
            ),
        )
    })?;
    let payload: PendingImport = serde_json::from_slice(&bytes).map_err(|error| {
        CommandError::system_fault(
            "agent_memory_lance_pending_decode_failed",
            format!(
                "Cadence could not decode the staged lance import at {}: {error}",
                path.display()
            ),
        )
    })?;
    if payload.schema_version != PENDING_IMPORT_SCHEMA_VERSION {
        return Err(CommandError::system_fault(
            "agent_memory_lance_pending_schema_mismatch",
            format!(
                "Staged lance import at {} has unsupported schema_version {}.",
                path.display(),
                payload.schema_version
            ),
        ));
    }
    Ok(payload)
}

fn pending_row_to_lance_row(row: PendingRow, project_id: &str) -> AgentMemoryRow {
    AgentMemoryRow {
        memory_id: row.memory_id,
        project_id: project_id.to_string(),
        agent_session_id: row.agent_session_id,
        scope: parse_scope(&row.scope),
        kind: parse_kind(&row.kind),
        text: row.text,
        text_hash: row.text_hash,
        review_state: parse_review_state(&row.review_state),
        enabled: row.enabled,
        confidence: row.confidence,
        source_run_id: row.source_run_id,
        source_item_ids: row.source_item_ids,
        diagnostic: row.diagnostic.map(|diagnostic| {
            crate::db::project_store::agent_core::AgentRunDiagnosticRecord {
                code: diagnostic.code,
                message: diagnostic.message,
            }
        }),
        created_at: row.created_at,
        updated_at: row.updated_at,
    }
}

fn parse_scope(value: &str) -> AgentMemoryScope {
    match value {
        "session" => AgentMemoryScope::Session,
        _ => AgentMemoryScope::Project,
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

fn parse_review_state(value: &str) -> AgentMemoryReviewState {
    match value {
        "approved" => AgentMemoryReviewState::Approved,
        "rejected" => AgentMemoryReviewState::Rejected,
        _ => AgentMemoryReviewState::Candidate,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use rusqlite_migration::{Migrations, M};

    fn legacy_migrations() -> Migrations<'static> {
        Migrations::new(vec![
            M::up(
                r#"
                CREATE TABLE projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL DEFAULT '',
                    description TEXT NOT NULL DEFAULT '',
                    milestone TEXT NOT NULL DEFAULT '',
                    total_phases INTEGER NOT NULL DEFAULT 0,
                    completed_phases INTEGER NOT NULL DEFAULT 0,
                    active_phase INTEGER NOT NULL DEFAULT 0,
                    branch TEXT,
                    runtime TEXT,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE meta (
                    id INTEGER PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                );
                CREATE TABLE agent_sessions (
                    project_id TEXT NOT NULL,
                    agent_session_id TEXT NOT NULL,
                    title TEXT NOT NULL DEFAULT '',
                    summary TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL DEFAULT 'active',
                    selected INTEGER NOT NULL DEFAULT 0,
                    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                    PRIMARY KEY (project_id, agent_session_id)
                );
                CREATE TABLE agent_memories (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    memory_id TEXT NOT NULL,
                    project_id TEXT NOT NULL,
                    agent_session_id TEXT,
                    scope_kind TEXT NOT NULL,
                    memory_kind TEXT NOT NULL,
                    text TEXT NOT NULL,
                    text_hash TEXT NOT NULL,
                    review_state TEXT NOT NULL,
                    enabled INTEGER NOT NULL DEFAULT 0,
                    confidence INTEGER,
                    source_run_id TEXT,
                    source_item_ids_json TEXT NOT NULL DEFAULT '[]',
                    diagnostic_json TEXT,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                "#,
            ),
        ])
    }

    #[test]
    fn migration_stages_rows_and_drops_table() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("state.db");
        let mut connection = Connection::open(&db_path).expect("open");

        legacy_migrations()
            .to_latest(&mut connection)
            .expect("legacy migrations");

        connection
            .execute(
                "INSERT INTO projects (id, name) VALUES ('project-1', 'Project 1')",
                [],
            )
            .expect("seed project");
        connection
            .execute(
                "INSERT INTO meta (id, project_id) VALUES (1, 'project-1')",
                [],
            )
            .expect("seed meta");
        connection
            .execute(
                r#"
                INSERT INTO agent_memories (
                    memory_id, project_id, agent_session_id, scope_kind, memory_kind, text,
                    text_hash, review_state, enabled, confidence, source_run_id,
                    source_item_ids_json, diagnostic_json, created_at, updated_at
                )
                VALUES (?1, ?2, NULL, 'project', 'decision', ?3, ?4, 'candidate', 0, 50,
                        NULL, '["message:1"]', NULL, ?5, ?5)
                "#,
                params![
                    "memory-1",
                    "project-1",
                    "Memory body",
                    "0".repeat(64),
                    "2026-04-26T00:00:00Z",
                ],
            )
            .expect("seed memory");

        // Now apply the drain hook by running the augmented migration set —
        // it sees the same DB at "user_version = previous" and only applies
        // the new step.
        let mut steps: Vec<M<'static>> = Vec::new();
        steps.push(M::up(""));
        // Use a fresh connection that hits user_version = 1 (legacy); add
        // the drain hook at version 2.
        let drain = Migrations::new(vec![
            M::up(""),
            M::up_with_hook("", super::migrate_agent_memories_to_lance_pending),
        ]);
        drain.to_latest(&mut connection).expect("drain hook");

        // Table should be gone.
        let still_there: i64 = connection
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE name = 'agent_memories'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(still_there, 0, "agent_memories should be dropped");

        // Pending file should exist with the row.
        let pending = pending_import_path(&db_path);
        let bytes = fs::read(&pending).expect("pending file");
        let payload: PendingImport = serde_json::from_slice(&bytes).expect("decode");
        assert_eq!(payload.schema_version, PENDING_IMPORT_SCHEMA_VERSION);
        assert_eq!(payload.project_id.as_deref(), Some("project-1"));
        assert_eq!(payload.rows.len(), 1);
        assert_eq!(payload.rows[0].memory_id, "memory-1");
        assert_eq!(payload.rows[0].text_hash.len(), 64);
    }

    #[test]
    fn migration_drops_table_when_no_rows() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("state.db");
        let mut connection = Connection::open(&db_path).expect("open");

        legacy_migrations()
            .to_latest(&mut connection)
            .expect("legacy migrations");

        connection
            .execute(
                "INSERT INTO projects (id, name) VALUES ('project-empty', 'Empty')",
                [],
            )
            .expect("seed project");
        connection
            .execute(
                "INSERT INTO meta (id, project_id) VALUES (1, 'project-empty')",
                [],
            )
            .expect("seed meta");

        let drain = Migrations::new(vec![
            M::up(""),
            M::up_with_hook("", super::migrate_agent_memories_to_lance_pending),
        ]);
        drain.to_latest(&mut connection).expect("drain hook");

        let still_there: i64 = connection
            .query_row(
                "SELECT COUNT(1) FROM sqlite_master WHERE name = 'agent_memories'",
                [],
                |row| row.get(0),
            )
            .expect("count");
        assert_eq!(still_there, 0);

        // Pending file should NOT exist when there were no rows to stage.
        let pending = pending_import_path(&db_path);
        assert!(!pending.exists());
    }

    #[test]
    fn drain_pending_into_lance_round_trips() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("state.db");
        let payload = PendingImport {
            schema_version: PENDING_IMPORT_SCHEMA_VERSION,
            project_id: Some("project-2".into()),
            rows: vec![PendingRow {
                memory_id: "memory-x".into(),
                agent_session_id: None,
                scope: "project".into(),
                kind: "decision".into(),
                text: "Imported memory".into(),
                text_hash: "1".repeat(64),
                review_state: "approved".into(),
                enabled: true,
                confidence: Some(80),
                source_run_id: Some("run-imported".into()),
                source_item_ids: vec!["message:1".into()],
                diagnostic: None,
                created_at: "2026-04-26T00:00:00Z".into(),
                updated_at: "2026-04-26T00:00:00Z".into(),
            }],
        };
        // The pending file lives next to the database; the database itself
        // does not need to exist on disk for the drain.
        fs::create_dir_all(dir.path()).unwrap();
        fs::write(
            pending_import_path(&db_path),
            serde_json::to_vec_pretty(&payload).unwrap(),
        )
        .expect("write pending");

        agent_memory_lance::reset_connection_cache_for_tests();
        let drained =
            drain_pending_into_lance(&db_path, "fallback").expect("drain succeeds");
        assert!(drained, "drain should report a payload was processed");

        // Pending file should be gone.
        assert!(!pending_import_path(&db_path).exists());

        // The Lance dataset should now hold the row.
        let store =
            agent_memory_lance::open_for_database_path(&db_path, "project-2");
        let approved = store.list_approved(None).expect("list approved");
        assert_eq!(approved.len(), 1);
        assert_eq!(approved[0].memory_id, "memory-x");
        assert_eq!(approved[0].project_id, "project-2");
        assert!(approved[0].enabled);
    }

    #[test]
    fn drain_no_op_when_no_pending_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("state.db");
        let drained =
            drain_pending_into_lance(&db_path, "anything").expect("no-op drain succeeds");
        assert!(!drained);
    }

    #[test]
    fn drain_rejects_unknown_schema_version() {
        let dir = tempfile::tempdir().expect("tempdir");
        let db_path = dir.path().join("state.db");
        fs::write(
            pending_import_path(&db_path),
            br#"{"schema_version": 99, "project_id": null, "rows": []}"#,
        )
        .unwrap();
        let err = drain_pending_into_lance(&db_path, "p")
            .expect_err("schema mismatch is fatal");
        assert_eq!(err.code, "agent_memory_lance_pending_schema_mismatch");
    }
}
