//! Phase 2.4 importers — one-shot legacy JSON → SQLite migrations for the small singleton and
//! registry stores: dictation settings, skill source settings, MCP registry, and the provider
//! model catalog cache.
//!
//! Each importer is idempotent and only runs when (a) the corresponding SQL table is empty and
//! (b) the legacy JSON file exists. After a successful write the legacy file is deleted.

use std::{fs, path::Path};

use rusqlite::{params, Connection, OptionalExtension};

use crate::commands::{CommandError, CommandResult};

/// Imports `dictation-settings.json` into the `dictation_settings` table.
pub fn import_legacy_dictation_settings(
    connection: &Connection,
    legacy_path: &Path,
) -> CommandResult<()> {
    if singleton_table_populated(connection, "dictation_settings")? {
        return Ok(());
    }
    if !legacy_path.exists() {
        return Ok(());
    }

    let payload = read_legacy_file(legacy_path, "dictation_settings_read_failed")?;
    let updated_at = extract_updated_at(&payload);
    upsert_singleton(connection, "dictation_settings", &payload, &updated_at)?;
    remove_legacy(legacy_path, "dictation_settings_legacy_cleanup_failed")
}

/// Imports `skill-sources.json` into the `skill_sources` table.
pub fn import_legacy_skill_sources(
    connection: &Connection,
    legacy_path: &Path,
) -> CommandResult<()> {
    if singleton_table_populated(connection, "skill_sources")? {
        return Ok(());
    }
    if !legacy_path.exists() {
        return Ok(());
    }

    let payload = read_legacy_file(legacy_path, "skill_source_settings_read_failed")?;
    let updated_at = extract_updated_at(&payload);
    upsert_singleton(connection, "skill_sources", &payload, &updated_at)?;
    remove_legacy(legacy_path, "skill_source_settings_legacy_cleanup_failed")
}

/// Imports `mcp-registry.json` into the `mcp_registry` table.
///
/// The legacy registry stores its servers as a JSON array under `servers`; each server becomes a
/// row keyed by its `id` so callers can read individual servers without parsing a giant blob.
pub fn import_legacy_mcp_registry(
    connection: &Connection,
    legacy_path: &Path,
) -> CommandResult<()> {
    if registry_table_populated(connection, "mcp_registry")? {
        return Ok(());
    }
    if !legacy_path.exists() {
        return Ok(());
    }

    let contents = fs::read_to_string(legacy_path).map_err(|error| {
        CommandError::retryable(
            "mcp_registry_read_failed",
            format!(
                "Cadence could not read legacy MCP registry at {}: {error}",
                legacy_path.display()
            ),
        )
    })?;
    let value: serde_json::Value = serde_json::from_str(&contents).map_err(|error| {
        CommandError::user_fixable(
            "mcp_registry_decode_failed",
            format!(
                "Cadence could not decode legacy MCP registry at {}: {error}",
                legacy_path.display()
            ),
        )
    })?;

    let updated_at = value
        .get("updatedAt")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(crate::auth::now_timestamp);

    let servers = value
        .get("servers")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();

    if servers.is_empty() {
        remove_legacy(legacy_path, "mcp_registry_legacy_cleanup_failed")?;
        return Ok(());
    }

    for server in &servers {
        let id = server
            .get("id")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "mcp_registry_decode_failed",
                    format!(
                        "Cadence could not import legacy MCP registry at {} because a server entry was missing `id`.",
                        legacy_path.display()
                    ),
                )
            })?;
        let payload = serde_json::to_string(server).map_err(|error| {
            CommandError::system_fault(
                "mcp_registry_serialize_failed",
                format!("Cadence could not re-encode MCP server `{id}`: {error}"),
            )
        })?;
        connection
            .execute(
                "INSERT INTO mcp_registry (server_id, payload, updated_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(server_id) DO UPDATE SET
                    payload = excluded.payload,
                    updated_at = excluded.updated_at",
                params![id, payload, updated_at],
            )
            .map_err(|error| {
                CommandError::retryable(
                    "mcp_registry_write_failed",
                    format!("Cadence could not import MCP server `{id}`: {error}"),
                )
            })?;
    }

    remove_legacy(legacy_path, "mcp_registry_legacy_cleanup_failed")
}

/// Imports `provider-model-catalogs.json` into the `provider_model_catalog_cache` table.
pub fn import_legacy_provider_model_catalog_cache(
    connection: &Connection,
    legacy_path: &Path,
) -> CommandResult<()> {
    if registry_table_populated(connection, "provider_model_catalog_cache")? {
        return Ok(());
    }
    if !legacy_path.exists() {
        return Ok(());
    }

    let contents = fs::read_to_string(legacy_path).map_err(|error| {
        CommandError::retryable(
            "provider_model_catalog_cache_read_failed",
            format!(
                "Cadence could not read legacy provider-model catalog cache at {}: {error}",
                legacy_path.display()
            ),
        )
    })?;
    let value: serde_json::Value = serde_json::from_str(&contents).map_err(|error| {
        CommandError::user_fixable(
            "provider_model_catalog_cache_decode_failed",
            format!(
                "Cadence could not decode legacy provider-model catalog cache at {}: {error}",
                legacy_path.display()
            ),
        )
    })?;

    let catalogs = value
        .get("catalogs")
        .and_then(serde_json::Value::as_object)
        .cloned()
        .unwrap_or_default();

    if catalogs.is_empty() {
        remove_legacy(legacy_path, "provider_model_catalog_cache_legacy_cleanup_failed")?;
        return Ok(());
    }

    for (profile_id, row_value) in catalogs {
        let fetched_at = row_value
            .get("fetchedAt")
            .or_else(|| row_value.get("fetched_at"))
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(crate::auth::now_timestamp);
        let payload = serde_json::to_string(&row_value).map_err(|error| {
            CommandError::system_fault(
                "provider_model_catalog_cache_serialize_failed",
                format!("Cadence could not re-encode provider-model catalog row for `{profile_id}`: {error}"),
            )
        })?;
        connection
            .execute(
                "INSERT INTO provider_model_catalog_cache (profile_id, payload, fetched_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(profile_id) DO UPDATE SET
                    payload = excluded.payload,
                    fetched_at = excluded.fetched_at",
                params![profile_id, payload, fetched_at],
            )
            .map_err(|error| {
                CommandError::retryable(
                    "provider_model_catalog_cache_write_failed",
                    format!("Cadence could not import catalog row `{profile_id}`: {error}"),
                )
            })?;
    }

    remove_legacy(legacy_path, "provider_model_catalog_cache_legacy_cleanup_failed")
}

fn singleton_table_populated(connection: &Connection, table: &str) -> CommandResult<bool> {
    let row: Option<i64> = connection
        .query_row(&format!("SELECT 1 FROM {table} WHERE id = 1"), [], |row| row.get(0))
        .optional()
        .map_err(|error| {
            CommandError::retryable(
                format!("{table}_read_failed"),
                format!("Cadence could not probe `{table}`: {error}"),
            )
        })?;
    Ok(row.is_some())
}

fn registry_table_populated(connection: &Connection, table: &str) -> CommandResult<bool> {
    let count: i64 = connection
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| row.get(0))
        .map_err(|error| {
            CommandError::retryable(
                format!("{table}_read_failed"),
                format!("Cadence could not probe `{table}`: {error}"),
            )
        })?;
    Ok(count > 0)
}

fn read_legacy_file(path: &Path, error_code: &'static str) -> CommandResult<String> {
    fs::read_to_string(path).map_err(|error| {
        CommandError::retryable(
            error_code,
            format!(
                "Cadence could not read legacy file at {}: {error}",
                path.display()
            ),
        )
    })
}

fn upsert_singleton(
    connection: &Connection,
    table: &str,
    payload: &str,
    updated_at: &str,
) -> CommandResult<()> {
    let sql = format!(
        "INSERT INTO {table} (id, payload, updated_at) VALUES (1, ?1, ?2)
         ON CONFLICT(id) DO UPDATE SET
            payload = excluded.payload,
            updated_at = excluded.updated_at"
    );
    connection
        .execute(&sql, params![payload, updated_at])
        .map_err(|error| {
            CommandError::retryable(
                format!("{table}_write_failed"),
                format!("Cadence could not write to `{table}`: {error}"),
            )
        })?;
    Ok(())
}

fn extract_updated_at(payload: &str) -> String {
    serde_json::from_str::<serde_json::Value>(payload)
        .ok()
        .as_ref()
        .and_then(|value| value.get("updatedAt"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
        .unwrap_or_else(crate::auth::now_timestamp)
}

fn remove_legacy(path: &Path, error_code: &'static str) -> CommandResult<()> {
    fs::remove_file(path).map_err(|error| {
        CommandError::retryable(
            error_code,
            format!(
                "Cadence imported {} into the global database but could not delete it: {error}",
                path.display()
            ),
        )
    })
}
