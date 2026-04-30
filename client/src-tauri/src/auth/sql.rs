use rusqlite::{params, Connection, OptionalExtension};

use super::{AuthFlowError, StoredOpenAiCodexSession};
use crate::commands::RuntimeAuthPhase;

pub fn load_openai_codex_session_by_account(
    connection: &Connection,
    account_id: &str,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    connection
        .query_row(
            "SELECT account_id, provider_id, session_id, access_token, refresh_token, expires_at, updated_at \
             FROM openai_codex_sessions WHERE account_id = ?1",
            params![account_id],
            map_row,
        )
        .optional()
        .map_err(|error| read_error(format!("Xero could not read openai_codex_sessions: {error}")))
}

pub fn load_latest_openai_codex_session(
    connection: &Connection,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    connection
        .query_row(
            "SELECT account_id, provider_id, session_id, access_token, refresh_token, expires_at, updated_at \
             FROM openai_codex_sessions ORDER BY updated_at DESC LIMIT 1",
            [],
            map_row,
        )
        .optional()
        .map_err(|error| read_error(format!("Xero could not read openai_codex_sessions: {error}")))
}

pub fn load_openai_codex_session_by_session_id(
    connection: &Connection,
    session_id: &str,
) -> Result<Option<StoredOpenAiCodexSession>, AuthFlowError> {
    connection
        .query_row(
            "SELECT account_id, provider_id, session_id, access_token, refresh_token, expires_at, updated_at \
             FROM openai_codex_sessions WHERE session_id = ?1 ORDER BY updated_at DESC LIMIT 1",
            params![session_id],
            map_row,
        )
        .optional()
        .map_err(|error| read_error(format!("Xero could not read openai_codex_sessions: {error}")))
}

pub fn upsert_openai_codex_session(
    connection: &Connection,
    session: &StoredOpenAiCodexSession,
) -> Result<(), AuthFlowError> {
    connection
        .execute(
            "INSERT INTO openai_codex_sessions (
                account_id, provider_id, session_id, access_token, refresh_token, expires_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(account_id) DO UPDATE SET
                provider_id = excluded.provider_id,
                session_id = excluded.session_id,
                access_token = excluded.access_token,
                refresh_token = excluded.refresh_token,
                expires_at = excluded.expires_at,
                updated_at = excluded.updated_at",
            params![
                session.account_id,
                session.provider_id,
                session.session_id,
                session.access_token,
                session.refresh_token,
                session.expires_at,
                session.updated_at,
            ],
        )
        .map_err(|error| {
            write_error(format!(
                "Xero could not write openai_codex_sessions: {error}"
            ))
        })?;
    Ok(())
}

pub fn remove_openai_codex_session(
    connection: &Connection,
    account_id: &str,
) -> Result<(), AuthFlowError> {
    connection
        .execute(
            "DELETE FROM openai_codex_sessions WHERE account_id = ?1",
            params![account_id],
        )
        .map_err(|error| {
            write_error(format!(
                "Xero could not delete openai_codex_sessions row: {error}"
            ))
        })?;
    Ok(())
}

pub fn clear_openai_codex_sessions(connection: &Connection) -> Result<(), AuthFlowError> {
    connection
        .execute("DELETE FROM openai_codex_sessions", [])
        .map_err(|error| {
            write_error(format!(
                "Xero could not clear openai_codex_sessions: {error}"
            ))
        })?;
    Ok(())
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredOpenAiCodexSession> {
    Ok(StoredOpenAiCodexSession {
        account_id: row.get(0)?,
        provider_id: row.get(1)?,
        session_id: row.get(2)?,
        access_token: row.get(3)?,
        refresh_token: row.get(4)?,
        expires_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn read_error(message: String) -> AuthFlowError {
    AuthFlowError::terminal("auth_store_read_failed", RuntimeAuthPhase::Failed, message)
}

fn write_error(message: String) -> AuthFlowError {
    AuthFlowError::terminal("auth_store_write_failed", RuntimeAuthPhase::Failed, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_db::migrations::migrations;

    fn open_in_memory() -> Connection {
        let mut connection = Connection::open_in_memory().expect("open in-memory db");
        connection
            .execute_batch("PRAGMA foreign_keys = ON;")
            .expect("enable foreign keys");
        migrations()
            .to_latest(&mut connection)
            .expect("walk migrations");
        connection
    }

    fn fixture(account_id: &str, session_id: &str, updated_at: &str) -> StoredOpenAiCodexSession {
        StoredOpenAiCodexSession {
            account_id: account_id.into(),
            provider_id: "openai_codex".into(),
            session_id: session_id.into(),
            access_token: "access".into(),
            refresh_token: "refresh".into(),
            expires_at: 1_900_000_000,
            updated_at: updated_at.into(),
        }
    }

    #[test]
    fn upsert_then_load_by_account_round_trips() {
        let connection = open_in_memory();
        let session = fixture("acct-1", "sess-1", "2026-01-01T00:00:00Z");
        upsert_openai_codex_session(&connection, &session).expect("upsert");
        let loaded = load_openai_codex_session_by_account(&connection, "acct-1")
            .expect("load")
            .expect("session present");
        assert_eq!(loaded.session_id, "sess-1");
    }

    #[test]
    fn latest_returns_most_recently_updated() {
        let connection = open_in_memory();
        upsert_openai_codex_session(&connection, &fixture("a", "s1", "2026-01-01T00:00:00Z"))
            .expect("upsert a");
        upsert_openai_codex_session(&connection, &fixture("b", "s2", "2026-02-01T00:00:00Z"))
            .expect("upsert b");
        let latest = load_latest_openai_codex_session(&connection)
            .expect("load")
            .expect("session present");
        assert_eq!(latest.account_id, "b");
    }

    #[test]
    fn remove_deletes_only_matching_row() {
        let connection = open_in_memory();
        upsert_openai_codex_session(&connection, &fixture("a", "s1", "2026-01-01T00:00:00Z"))
            .expect("upsert a");
        upsert_openai_codex_session(&connection, &fixture("b", "s2", "2026-02-01T00:00:00Z"))
            .expect("upsert b");
        remove_openai_codex_session(&connection, "a").expect("remove a");
        assert!(load_openai_codex_session_by_account(&connection, "a")
            .expect("load")
            .is_none());
        assert!(load_openai_codex_session_by_account(&connection, "b")
            .expect("load")
            .is_some());
    }

    #[test]
    fn clear_removes_all_rows() {
        let connection = open_in_memory();
        upsert_openai_codex_session(&connection, &fixture("a", "s1", "2026-01-01T00:00:00Z"))
            .expect("upsert");
        clear_openai_codex_sessions(&connection).expect("clear");
        assert!(load_latest_openai_codex_session(&connection)
            .expect("load")
            .is_none());
    }
}
