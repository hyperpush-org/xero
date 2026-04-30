use rusqlite::{params, Connection, OptionalExtension};

use crate::commands::{CommandError, CommandResult};

use super::{ProviderCredentialKind, ProviderCredentialRecord, ProviderCredentialsSnapshot};

const SELECT_COLUMNS: &str = "provider_id, kind, api_key, oauth_account_id, oauth_session_id, \
                              oauth_access_token, oauth_refresh_token, oauth_expires_at, \
                              base_url, api_version, region, scope_project_id, \
                              default_model_id, updated_at";

pub fn load_all_provider_credentials(
    connection: &Connection,
) -> CommandResult<ProviderCredentialsSnapshot> {
    let mut stmt = connection
        .prepare(&format!(
            "SELECT {SELECT_COLUMNS} FROM provider_credentials ORDER BY provider_id"
        ))
        .map_err(|error| {
            CommandError::retryable(
                "provider_credentials_read_failed",
                format!("Xero could not prepare provider_credentials read: {error}"),
            )
        })?;

    let rows = stmt.query_map([], map_row).map_err(|error| {
        CommandError::retryable(
            "provider_credentials_read_failed",
            format!("Xero could not query provider_credentials: {error}"),
        )
    })?;

    let mut records = Vec::new();
    for row in rows {
        let row = row.map_err(|error| {
            CommandError::retryable(
                "provider_credentials_read_failed",
                format!("Xero could not decode provider_credentials row: {error}"),
            )
        })?;
        // Skip rows whose `kind` value isn't one we recognize. Should never
        // happen in practice (the CHECK constraint blocks it at write time),
        // but guards against future schema drift.
        if let Some(record) = row {
            records.push(record);
        }
    }

    Ok(records)
}

pub fn load_provider_credential(
    connection: &Connection,
    provider_id: &str,
) -> CommandResult<Option<ProviderCredentialRecord>> {
    connection
        .query_row(
            &format!("SELECT {SELECT_COLUMNS} FROM provider_credentials WHERE provider_id = ?1"),
            params![provider_id],
            map_row,
        )
        .optional()
        .map_err(|error| {
            CommandError::retryable(
                "provider_credentials_read_failed",
                format!("Xero could not read provider_credentials row: {error}"),
            )
        })
        .map(|maybe_row| maybe_row.flatten())
}

pub fn upsert_provider_credential(
    connection: &Connection,
    record: &ProviderCredentialRecord,
) -> CommandResult<()> {
    connection
        .execute(
            "INSERT INTO provider_credentials (
                provider_id, kind, api_key,
                oauth_account_id, oauth_session_id,
                oauth_access_token, oauth_refresh_token, oauth_expires_at,
                base_url, api_version, region, scope_project_id,
                default_model_id, updated_at
            ) VALUES (
                ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14
            )
            ON CONFLICT(provider_id) DO UPDATE SET
                kind = excluded.kind,
                api_key = excluded.api_key,
                oauth_account_id = excluded.oauth_account_id,
                oauth_session_id = excluded.oauth_session_id,
                oauth_access_token = excluded.oauth_access_token,
                oauth_refresh_token = excluded.oauth_refresh_token,
                oauth_expires_at = excluded.oauth_expires_at,
                base_url = excluded.base_url,
                api_version = excluded.api_version,
                region = excluded.region,
                scope_project_id = excluded.scope_project_id,
                default_model_id = excluded.default_model_id,
                updated_at = excluded.updated_at",
            params![
                record.provider_id,
                record.kind.as_sql_str(),
                record.api_key,
                record.oauth_account_id,
                record.oauth_session_id,
                record.oauth_access_token,
                record.oauth_refresh_token,
                record.oauth_expires_at,
                record.base_url,
                record.api_version,
                record.region,
                record.project_id,
                record.default_model_id,
                record.updated_at,
            ],
        )
        .map_err(|error| {
            CommandError::retryable(
                "provider_credentials_write_failed",
                format!("Xero could not write provider_credentials row: {error}"),
            )
        })?;
    Ok(())
}

pub fn delete_provider_credential(connection: &Connection, provider_id: &str) -> CommandResult<()> {
    connection
        .execute(
            "DELETE FROM provider_credentials WHERE provider_id = ?1",
            params![provider_id],
        )
        .map_err(|error| {
            CommandError::retryable(
                "provider_credentials_write_failed",
                format!("Xero could not delete provider_credentials row: {error}"),
            )
        })?;
    Ok(())
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Option<ProviderCredentialRecord>> {
    let provider_id: String = row.get(0)?;
    let kind_str: String = row.get(1)?;
    let Some(kind) = ProviderCredentialKind::from_sql_str(&kind_str) else {
        return Ok(None);
    };
    Ok(Some(ProviderCredentialRecord {
        provider_id,
        kind,
        api_key: row.get(2)?,
        oauth_account_id: row.get(3)?,
        oauth_session_id: row.get(4)?,
        oauth_access_token: row.get(5)?,
        oauth_refresh_token: row.get(6)?,
        oauth_expires_at: row.get(7)?,
        base_url: row.get(8)?,
        api_version: row.get(9)?,
        region: row.get(10)?,
        project_id: row.get(11)?,
        default_model_id: row.get(12)?,
        updated_at: row.get(13)?,
    }))
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
            .expect("walk migrations to latest");
        connection
    }

    fn api_key_record(provider_id: &str, api_key: &str) -> ProviderCredentialRecord {
        ProviderCredentialRecord {
            provider_id: provider_id.into(),
            kind: ProviderCredentialKind::ApiKey,
            api_key: Some(api_key.into()),
            oauth_account_id: None,
            oauth_session_id: None,
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_expires_at: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            default_model_id: Some("openai/gpt-4.1-mini".into()),
            updated_at: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn load_all_returns_empty_for_fresh_db() {
        let connection = open_in_memory();
        let records = load_all_provider_credentials(&connection).expect("load");
        assert!(records.is_empty());
    }

    #[test]
    fn upsert_then_load_round_trips() {
        let connection = open_in_memory();
        let record = api_key_record("openrouter", "sk-or-test");
        upsert_provider_credential(&connection, &record).expect("upsert");

        let loaded = load_all_provider_credentials(&connection).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0], record);

        let by_provider = load_provider_credential(&connection, "openrouter")
            .expect("load by provider")
            .expect("row present");
        assert_eq!(by_provider, record);
    }

    #[test]
    fn upsert_replaces_existing_row() {
        let connection = open_in_memory();
        upsert_provider_credential(&connection, &api_key_record("openrouter", "sk-old"))
            .expect("first upsert");
        upsert_provider_credential(&connection, &api_key_record("openrouter", "sk-new"))
            .expect("second upsert");

        let loaded = load_all_provider_credentials(&connection).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].api_key.as_deref(), Some("sk-new"));
    }

    #[test]
    fn delete_removes_only_matching_row() {
        let connection = open_in_memory();
        upsert_provider_credential(&connection, &api_key_record("openrouter", "sk-or"))
            .expect("upsert openrouter");
        upsert_provider_credential(&connection, &api_key_record("anthropic", "sk-an"))
            .expect("upsert anthropic");

        delete_provider_credential(&connection, "openrouter").expect("delete openrouter");
        let loaded = load_all_provider_credentials(&connection).expect("load");
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].provider_id, "anthropic");
    }

    #[test]
    fn check_constraint_rejects_api_key_without_secret() {
        let connection = open_in_memory();
        let mut record = api_key_record("openrouter", "sk-test");
        record.api_key = None;
        let result = upsert_provider_credential(&connection, &record);
        assert!(
            result.is_err(),
            "missing api_key for kind=api_key must error"
        );
    }

    #[test]
    fn check_constraint_rejects_oauth_without_account() {
        let connection = open_in_memory();
        let record = ProviderCredentialRecord {
            provider_id: "openai_codex".into(),
            kind: ProviderCredentialKind::OAuthSession,
            api_key: None,
            oauth_account_id: None,
            oauth_session_id: Some("sess-1".into()),
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_expires_at: None,
            base_url: None,
            api_version: None,
            region: None,
            project_id: None,
            default_model_id: None,
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        let result = upsert_provider_credential(&connection, &record);
        assert!(
            result.is_err(),
            "oauth row without account_id must violate CHECK"
        );
    }

    #[test]
    fn local_kind_round_trips_without_secret_columns() {
        let connection = open_in_memory();
        let record = ProviderCredentialRecord {
            provider_id: "ollama".into(),
            kind: ProviderCredentialKind::Local,
            api_key: None,
            oauth_account_id: None,
            oauth_session_id: None,
            oauth_access_token: None,
            oauth_refresh_token: None,
            oauth_expires_at: None,
            base_url: Some("http://localhost:11434".into()),
            api_version: None,
            region: None,
            project_id: None,
            default_model_id: None,
            updated_at: "2026-01-01T00:00:00Z".into(),
        };
        upsert_provider_credential(&connection, &record).expect("upsert local");
        let loaded = load_provider_credential(&connection, "ollama")
            .expect("load")
            .expect("row present");
        assert_eq!(loaded, record);
    }
}
