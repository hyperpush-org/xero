use std::path::Path;

use rusqlite::{params, Connection};

use crate::{global_db::open_global_database, notifications::NotificationAdapterError};

use super::file_store::{
    NotificationCredentialStoreEntry, NotificationCredentialStoreFile,
    NotificationInboundCursorEntry,
};

pub(crate) fn open_db(path: &Path) -> Result<Connection, NotificationAdapterError> {
    open_global_database(path).map_err(|error| {
        NotificationAdapterError::new(
            error.code,
            format!(
                "Xero could not open the notification credential store database at {}: {}",
                path.display(),
                error.message
            ),
            error.retryable,
        )
    })
}

pub(crate) fn load_store(
    connection: &Connection,
) -> Result<NotificationCredentialStoreFile, NotificationAdapterError> {
    let routes = load_routes(connection)?;
    let inbound_cursors = load_inbound_cursors(connection)?;
    Ok(NotificationCredentialStoreFile {
        routes,
        inbound_cursors,
    })
}

fn load_routes(
    connection: &Connection,
) -> Result<Vec<NotificationCredentialStoreEntry>, NotificationAdapterError> {
    let mut stmt = connection
        .prepare(
            "SELECT project_id, route_id, route_kind, bot_token, chat_id, webhook_url \
             FROM notification_credentials ORDER BY project_id, route_id",
        )
        .map_err(map_read_error)?;

    let rows = stmt
        .query_map([], |row| {
            Ok(NotificationCredentialStoreEntry {
                project_id: row.get(0)?,
                route_id: row.get(1)?,
                route_kind: row.get(2)?,
                bot_token: row.get(3)?,
                chat_id: row.get(4)?,
                webhook_url: row.get(5)?,
            })
        })
        .map_err(map_read_error)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(map_read_error)?);
    }
    Ok(entries)
}

fn load_inbound_cursors(
    connection: &Connection,
) -> Result<Vec<NotificationInboundCursorEntry>, NotificationAdapterError> {
    let mut stmt = connection
        .prepare(
            "SELECT project_id, route_id, route_kind, cursor, updated_at \
             FROM notification_inbound_cursors ORDER BY project_id, route_id",
        )
        .map_err(map_read_error)?;

    let rows = stmt
        .query_map([], |row| {
            Ok(NotificationInboundCursorEntry {
                project_id: row.get(0)?,
                route_id: row.get(1)?,
                route_kind: row.get(2)?,
                cursor: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })
        .map_err(map_read_error)?;

    let mut entries = Vec::new();
    for row in rows {
        entries.push(row.map_err(map_read_error)?);
    }
    Ok(entries)
}

pub(crate) fn write_store(
    connection: &mut Connection,
    store: &NotificationCredentialStoreFile,
) -> Result<(), NotificationAdapterError> {
    let tx = connection.transaction().map_err(map_write_error)?;

    tx.execute("DELETE FROM notification_credentials", [])
        .map_err(map_write_error)?;
    for entry in &store.routes {
        tx.execute(
            "INSERT INTO notification_credentials (
                project_id, route_id, route_kind, bot_token, chat_id, webhook_url, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            params![
                entry.project_id,
                entry.route_id,
                entry.route_kind,
                entry.bot_token,
                entry.chat_id,
                entry.webhook_url,
            ],
        )
        .map_err(map_write_error)?;
    }

    tx.execute("DELETE FROM notification_inbound_cursors", [])
        .map_err(map_write_error)?;
    for entry in &store.inbound_cursors {
        tx.execute(
            "INSERT INTO notification_inbound_cursors (
                project_id, route_id, route_kind, cursor, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                entry.project_id,
                entry.route_id,
                entry.route_kind,
                entry.cursor,
                entry.updated_at,
            ],
        )
        .map_err(map_write_error)?;
    }

    tx.commit().map_err(map_write_error)?;
    Ok(())
}

fn map_read_error(error: rusqlite::Error) -> NotificationAdapterError {
    NotificationAdapterError::credentials_read_failed(format!(
        "Xero could not read notification credentials from the global database: {error}"
    ))
}

fn map_write_error(error: rusqlite::Error) -> NotificationAdapterError {
    NotificationAdapterError::new(
        "notification_adapter_credentials_write_failed",
        format!("Xero could not write notification credentials to the global database: {error}"),
        true,
    )
}
