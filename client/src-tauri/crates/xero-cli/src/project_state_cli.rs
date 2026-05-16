use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::{json, Value as JsonValue};

use super::{
    cli_app_data_root, global_database_path, now_timestamp, project_cli, response, take_bool_flag,
    take_help, take_option, workspace_project_database_path, CliError, CliResponse, GlobalOptions,
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectStateBackupEntry {
    backup_id: String,
    created_at: Option<String>,
    file_count: Option<usize>,
    byte_count: Option<u64>,
    manifest_present: bool,
    pre_restore: bool,
    backup_location: String,
    manifest_location: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectStateRepairDiagnostic {
    code: String,
    message: String,
    severity: String,
}

pub(crate) fn dispatch_project_state(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("list") | Some("backups") => command_project_state_list(globals, args[1..].to_vec()),
        Some("backup") | Some("create") => {
            command_project_state_backup(globals, args[1..].to_vec())
        }
        Some("restore") => command_project_state_restore(globals, args[1..].to_vec()),
        Some("repair") => command_project_state_repair(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero project-state list|backup|restore|repair [--project-id ID]\nUses the same OS app-data project-state layout as the desktop maintenance commands.",
            json!({ "command": "project-state" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown project-state command `{other}`. Use list, backup, restore, or repair."
        ))),
    }
}

pub(crate) fn dispatch_wipe(
    globals: GlobalOptions,
    args: Vec<String>,
) -> Result<CliResponse, CliError> {
    match args.first().map(String::as_str) {
        Some("project") => command_wipe_project(globals, args[1..].to_vec()),
        Some("all") => command_wipe_all(globals, args[1..].to_vec()),
        Some("--help") | Some("-h") | None => Ok(response(
            &globals,
            "Usage: xero wipe project PROJECT_ID --yes --confirm PROJECT_ID | xero wipe all --yes --confirm DELETE-XERO-APP-DATA",
            json!({ "command": "wipe" }),
        )),
        Some(other) => Err(CliError::usage(format!(
            "Unknown wipe command `{other}`. Use project or all."
        ))),
    }
}

fn command_project_state_list(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project-state list [--project-id ID]",
            json!({ "command": "project-state list" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    validate_project_id(&project_id)?;
    reject_project_state_unknown_options(&args)?;
    let backups = list_backups(&globals, &project_id)?;
    let text = if backups.is_empty() {
        format!("No project-state backups found for `{project_id}`.")
    } else {
        backups
            .iter()
            .map(|entry| {
                format!(
                    "{:<32} files={:<4} bytes={:<8} created={}",
                    entry.backup_id,
                    entry.file_count.unwrap_or(0),
                    entry.byte_count.unwrap_or(0),
                    entry.created_at.as_deref().unwrap_or("unknown")
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    Ok(response(
        &globals,
        text,
        json!({
            "schema": "xero.project_state_backup_list_command.v1",
            "projectId": project_id,
            "storageScope": "os_app_data",
            "backups": backups,
            "uiDeferred": true
        }),
    ))
}

fn command_project_state_backup(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project-state backup [--project-id ID] [--backup-id ID]",
            json!({ "command": "project-state backup" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    validate_project_id(&project_id)?;
    let created_at = now_timestamp();
    let backup_id = take_option(&mut args, "--backup-id")?
        .unwrap_or_else(|| format!("backup-{}", sanitize_backup_id(&created_at)));
    validate_backup_id(&backup_id)?;
    reject_project_state_unknown_options(&args)?;

    let database_path = workspace_project_database_path(&globals, &project_id);
    if !database_path.exists() {
        return Err(CliError::user_fixable(
            "xero_cli_project_state_backup_database_missing",
            format!(
                "Project `{project_id}` has no state database at `{}`.",
                database_path.display()
            ),
        ));
    }
    let connection = project_cli::project_connection(&globals, &project_id)?;
    checkpoint_project_database(&connection)?;
    drop(connection);

    let backup_dir = backup_dir(&globals, &project_id, &backup_id);
    if backup_dir.exists() {
        return Err(CliError::user_fixable(
            "xero_cli_project_state_backup_exists",
            format!("Backup `{backup_id}` already exists for project `{project_id}`."),
        ));
    }
    fs::create_dir_all(&backup_dir).map_err(|error| {
        io_error(
            "xero_cli_project_state_backup_dir_failed",
            format!(
                "Could not create backup directory `{}`: {error}",
                backup_dir.display()
            ),
        )
    })?;

    copy_file_if_exists(&database_path, &backup_dir.join("state.db"))?;
    copy_file_if_exists(
        &database_path.with_extension("db-wal"),
        &backup_dir.join("state.db-wal"),
    )?;
    copy_file_if_exists(
        &database_path.with_extension("db-shm"),
        &backup_dir.join("state.db-shm"),
    )?;
    let lance_dir = lance_dir_for_database(&database_path);
    if lance_dir.exists() {
        copy_dir_recursive(&lance_dir, &backup_dir.join("lance"))?;
    }

    let (file_count, byte_count) = directory_metrics(&backup_dir)?;
    let manifest_location = backup_dir.join("manifest.json");
    let manifest = json!({
        "schema": "xero.project_state_backup.v1",
        "backupId": backup_id,
        "projectId": project_id,
        "createdAt": created_at,
        "databasePath": database_path,
        "lanceDir": lance_dir,
        "fileCount": file_count,
        "byteCount": byte_count
    });
    fs::write(
        &manifest_location,
        serde_json::to_vec_pretty(&manifest).map_err(|error| {
            CliError::system_fault(
                "xero_cli_project_state_backup_manifest_encode_failed",
                format!("Could not encode backup manifest: {error}"),
            )
        })?,
    )
    .map_err(|error| {
        io_error(
            "xero_cli_project_state_backup_manifest_write_failed",
            format!(
                "Could not write backup manifest `{}`: {error}",
                manifest_location.display()
            ),
        )
    })?;

    Ok(response(
        &globals,
        format!("Created project-state backup `{backup_id}` for `{project_id}`."),
        json!({
            "schema": "xero.project_state_backup_command.v1",
            "projectId": project_id,
            "backupId": backup_id,
            "createdAt": created_at,
            "fileCount": file_count,
            "byteCount": byte_count,
            "storageScope": "os_app_data",
            "backupLocation": backup_location(&backup_id),
            "manifestLocation": manifest_location_for(&backup_id),
            "uiDeferred": true
        }),
    ))
}

fn command_project_state_restore(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project-state restore BACKUP_ID [--project-id ID] --yes",
            json!({ "command": "project-state restore" }),
        ));
    }
    let yes = take_bool_flag(&mut args, "--yes");
    let backup_id = take_option(&mut args, "--backup-id")?
        .or_else(|| take_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing backup id."))?;
    validate_backup_id(&backup_id)?;
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    validate_project_id(&project_id)?;
    reject_project_state_unknown_options(&args)?;
    if !yes {
        return Err(CliError::usage(
            "Restoring project state requires `--yes` after choosing a backup.",
        ));
    }

    let database_path = workspace_project_database_path(&globals, &project_id);
    let selected_backup_dir = backup_dir(&globals, &project_id, &backup_id);
    let backup_database = selected_backup_dir.join("state.db");
    if !backup_database.exists() {
        return Err(CliError::user_fixable(
            "xero_cli_project_state_backup_missing_database",
            format!("Backup `{backup_id}` does not contain state.db."),
        ));
    }
    let restored_at = now_timestamp();
    let pre_restore_backup_id = format!("pre-restore-{}", sanitize_backup_id(&restored_at));
    let pre_restore_dir =
        next_available_backup_dir(&backup_dir(&globals, &project_id, &pre_restore_backup_id));
    snapshot_current_state(&database_path, &pre_restore_dir)?;

    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            io_error(
                "xero_cli_project_state_restore_parent_failed",
                format!(
                    "Could not create project-state directory `{}`: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    remove_file_if_exists(&database_path)?;
    remove_file_if_exists(&database_path.with_extension("db-wal"))?;
    remove_file_if_exists(&database_path.with_extension("db-shm"))?;
    copy_file_if_exists(&backup_database, &database_path)?;
    copy_file_if_exists(
        &selected_backup_dir.join("state.db-wal"),
        &database_path.with_extension("db-wal"),
    )?;
    copy_file_if_exists(
        &selected_backup_dir.join("state.db-shm"),
        &database_path.with_extension("db-shm"),
    )?;
    let lance_dir = lance_dir_for_database(&database_path);
    if lance_dir.exists() {
        fs::remove_dir_all(&lance_dir).map_err(|error| {
            io_error(
                "xero_cli_project_state_restore_lance_remove_failed",
                format!(
                    "Could not remove Lance state `{}`: {error}",
                    lance_dir.display()
                ),
            )
        })?;
    }
    let backup_lance = selected_backup_dir.join("lance");
    if backup_lance.exists() {
        copy_dir_recursive(&backup_lance, &lance_dir)?;
    }

    let pre_restore_backup_id = pre_restore_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("pre-restore-backup")
        .to_owned();
    Ok(response(
        &globals,
        format!("Restored project `{project_id}` from `{backup_id}`."),
        json!({
            "schema": "xero.project_state_restore_command.v1",
            "projectId": project_id,
            "backupId": backup_id,
            "restoredAt": restored_at,
            "preRestoreBackupId": pre_restore_backup_id,
            "storageScope": "os_app_data",
            "uiDeferred": true
        }),
    ))
}

fn command_project_state_repair(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero project-state repair [--project-id ID]",
            json!({ "command": "project-state repair" }),
        ));
    }
    let project_id = project_cli::take_project_id_option_or_selected(&globals, &mut args)?;
    validate_project_id(&project_id)?;
    reject_project_state_unknown_options(&args)?;
    let checked_at = now_timestamp();
    let connection = project_cli::project_connection(&globals, &project_id)?;
    checkpoint_project_database(&connection)?;
    let integrity = connection
        .query_row("PRAGMA integrity_check;", [], |row| row.get::<_, String>(0))
        .unwrap_or_else(|error| format!("unavailable: {error}"));
    let mut diagnostics = Vec::new();
    if integrity != "ok" {
        diagnostics.push(ProjectStateRepairDiagnostic {
            code: "project_state_repair_sqlite_integrity".into(),
            message: format!("SQLite integrity check returned `{integrity}`."),
            severity: "error".into(),
        });
    }
    let database_path = workspace_project_database_path(&globals, &project_id);
    let lance_dir = lance_dir_for_database(&database_path);
    let lance_status = if lance_dir.exists() {
        "present"
    } else {
        "not_present"
    };

    Ok(response(
        &globals,
        format!(
            "Project-state repair checked `{project_id}`: sqlite_checkpointed=true integrity={integrity}."
        ),
        json!({
            "schema": "xero.project_state_repair_command.v1",
            "projectId": project_id,
            "checkedAt": checked_at,
            "sqliteCheckpointed": true,
            "outboxInspectedCount": 0,
            "outboxReconciledCount": 0,
            "outboxFailedCount": 0,
            "handoffInspectedCount": 0,
            "handoffRepairedCount": 0,
            "handoffFailedCount": 0,
            "projectRecordHealthStatus": lance_status,
            "agentMemoryHealthStatus": lance_status,
            "diagnostics": diagnostics,
            "storageScope": "os_app_data",
            "uiDeferred": true
        }),
    ))
}

fn command_wipe_project(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero wipe project PROJECT_ID --yes --confirm PROJECT_ID",
            json!({ "command": "wipe project" }),
        ));
    }
    let yes = take_bool_flag(&mut args, "--yes");
    let confirm = take_option(&mut args, "--confirm")?;
    let project_id = take_option(&mut args, "--project-id")?
        .or_else(|| take_positional(&mut args))
        .ok_or_else(|| CliError::usage("Missing project id."))?;
    validate_project_id(&project_id)?;
    reject_project_state_unknown_options(&args)?;
    if !yes || confirm.as_deref() != Some(project_id.as_str()) {
        return Err(CliError::usage(
            "Project data wipe requires `--yes --confirm PROJECT_ID`.",
        ));
    }
    remove_project_from_global_registry(&globals, &project_id)?;
    let project_dir = workspace_project_database_path(&globals, &project_id)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| {
            cli_app_data_root(&globals)
                .join("projects")
                .join(&project_id)
        });
    let directory_removed = remove_dir_if_present(&project_dir)?;
    Ok(response(
        &globals,
        format!("Wiped app-data state for project `{project_id}`."),
        json!({
            "schema": "xero.wipe_project_data_command.v1",
            "projectId": project_id,
            "directoryRemoved": directory_removed
        }),
    ))
}

fn command_wipe_all(
    globals: GlobalOptions,
    mut args: Vec<String>,
) -> Result<CliResponse, CliError> {
    if take_help(&args) {
        return Ok(response(
            &globals,
            "Usage: xero wipe all --yes --confirm DELETE-XERO-APP-DATA",
            json!({ "command": "wipe all" }),
        ));
    }
    let yes = take_bool_flag(&mut args, "--yes");
    let confirm = take_option(&mut args, "--confirm")?;
    reject_project_state_unknown_options(&args)?;
    if !yes || confirm.as_deref() != Some("DELETE-XERO-APP-DATA") {
        return Err(CliError::usage(
            "Full app-data wipe requires `--yes --confirm DELETE-XERO-APP-DATA`.",
        ));
    }
    let app_data_root = cli_app_data_root(&globals);
    let directory_removed = remove_dir_contents_if_present(&app_data_root)?;
    Ok(response(
        &globals,
        format!(
            "Wiped Xero app-data contents at `{}`.",
            app_data_root.display()
        ),
        json!({
            "schema": "xero.wipe_all_data_command.v1",
            "directoryRemoved": directory_removed
        }),
    ))
}

fn list_backups(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<Vec<ProjectStateBackupEntry>, CliError> {
    let backups_root = project_backups_root(globals, project_id);
    if !backups_root.exists() {
        return Ok(Vec::new());
    }
    let mut backups = Vec::new();
    for entry in fs::read_dir(&backups_root).map_err(|error| {
        io_error(
            "xero_cli_project_state_backup_list_failed",
            format!(
                "Could not list backups in `{}`: {error}",
                backups_root.display()
            ),
        )
    })? {
        let entry = entry.map_err(|error| {
            io_error(
                "xero_cli_project_state_backup_list_entry_failed",
                format!("Could not read a backup entry: {error}"),
            )
        })?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(backup_id) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if backup_id.is_empty() {
            continue;
        }
        let manifest_path = path.join("manifest.json");
        let manifest = read_json_if_present(&manifest_path);
        backups.push(ProjectStateBackupEntry {
            backup_location: backup_location(&backup_id),
            manifest_location: manifest_location_for(&backup_id),
            pre_restore: backup_id.starts_with("pre-restore-"),
            backup_id,
            created_at: manifest
                .as_ref()
                .and_then(|value| value.get("createdAt"))
                .and_then(JsonValue::as_str)
                .map(str::to_owned),
            file_count: manifest
                .as_ref()
                .and_then(|value| value.get("fileCount"))
                .and_then(JsonValue::as_u64)
                .map(|value| value as usize),
            byte_count: manifest
                .as_ref()
                .and_then(|value| value.get("byteCount"))
                .and_then(JsonValue::as_u64),
            manifest_present: manifest_path.exists(),
        });
    }
    backups.sort_by(|left, right| match (&right.created_at, &left.created_at) {
        (Some(a), Some(b)) => a.cmp(b),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => right.backup_id.cmp(&left.backup_id),
    });
    Ok(backups)
}

fn checkpoint_project_database(connection: &Connection) -> Result<(), CliError> {
    connection
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_project_state_checkpoint_failed",
                format!("Could not checkpoint project state before maintenance: {error}"),
            )
        })
}

fn remove_project_from_global_registry(
    globals: &GlobalOptions,
    project_id: &str,
) -> Result<(), CliError> {
    let database_path = global_database_path(globals);
    if !database_path.exists() {
        return Ok(());
    }
    let connection = Connection::open(&database_path).map_err(|error| {
        CliError::system_fault(
            "xero_cli_wipe_registry_open_failed",
            format!(
                "Could not open global registry `{}`: {error}",
                database_path.display()
            ),
        )
    })?;
    if table_exists(&connection, "repositories")? {
        connection
            .execute(
                "DELETE FROM repositories WHERE project_id = ?1",
                params![project_id],
            )
            .map_err(|error| {
                CliError::system_fault(
                    "xero_cli_wipe_registry_write_failed",
                    format!("Could not remove repository rows for `{project_id}`: {error}"),
                )
            })?;
    }
    if table_exists(&connection, "projects")? {
        connection
            .execute("DELETE FROM projects WHERE id = ?1", params![project_id])
            .map_err(|error| {
                CliError::system_fault(
                    "xero_cli_wipe_registry_write_failed",
                    format!("Could not remove project row `{project_id}`: {error}"),
                )
            })?;
    }
    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, CliError> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            params![table],
            |row| row.get::<_, bool>(0),
        )
        .optional()
        .map(|value| value.unwrap_or(false))
        .map_err(|error| {
            CliError::system_fault(
                "xero_cli_project_state_table_probe_failed",
                format!("Could not inspect project-state schema: {error}"),
            )
        })
}

fn snapshot_current_state(database_path: &Path, backup_dir: &Path) -> Result<(), CliError> {
    fs::create_dir_all(backup_dir).map_err(|error| {
        io_error(
            "xero_cli_project_state_pre_restore_dir_failed",
            format!(
                "Could not create pre-restore backup `{}`: {error}",
                backup_dir.display()
            ),
        )
    })?;
    copy_file_if_exists(database_path, &backup_dir.join("state.db"))?;
    copy_file_if_exists(
        &database_path.with_extension("db-wal"),
        &backup_dir.join("state.db-wal"),
    )?;
    copy_file_if_exists(
        &database_path.with_extension("db-shm"),
        &backup_dir.join("state.db-shm"),
    )?;
    let lance_dir = lance_dir_for_database(database_path);
    if lance_dir.exists() {
        copy_dir_recursive(&lance_dir, &backup_dir.join("lance"))?;
    }
    Ok(())
}

fn copy_file_if_exists(source: &Path, target: &Path) -> Result<(), CliError> {
    if !source.exists() {
        return Ok(());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            io_error(
                "xero_cli_project_state_copy_parent_failed",
                format!("Could not create `{}`: {error}", parent.display()),
            )
        })?;
    }
    fs::copy(source, target).map_err(|error| {
        io_error(
            "xero_cli_project_state_copy_failed",
            format!(
                "Could not copy `{}` to `{}`: {error}",
                source.display(),
                target.display()
            ),
        )
    })?;
    Ok(())
}

fn copy_dir_recursive(source: &Path, target: &Path) -> Result<(), CliError> {
    fs::create_dir_all(target).map_err(|error| {
        io_error(
            "xero_cli_project_state_copy_dir_failed",
            format!("Could not create `{}`: {error}", target.display()),
        )
    })?;
    for entry in fs::read_dir(source).map_err(|error| {
        io_error(
            "xero_cli_project_state_copy_dir_read_failed",
            format!("Could not read `{}`: {error}", source.display()),
        )
    })? {
        let entry = entry.map_err(|error| {
            io_error(
                "xero_cli_project_state_copy_dir_entry_failed",
                format!("Could not inspect `{}`: {error}", source.display()),
            )
        })?;
        let source_path = entry.path();
        let target_path = target.join(entry.file_name());
        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &target_path)?;
        } else {
            copy_file_if_exists(&source_path, &target_path)?;
        }
    }
    Ok(())
}

fn directory_metrics(path: &Path) -> Result<(usize, u64), CliError> {
    if !path.exists() {
        return Ok((0, 0));
    }
    let mut file_count = 0usize;
    let mut byte_count = 0u64;
    for entry in fs::read_dir(path).map_err(|error| {
        io_error(
            "xero_cli_project_state_metric_read_failed",
            format!("Could not inspect `{}`: {error}", path.display()),
        )
    })? {
        let entry = entry.map_err(|error| {
            io_error(
                "xero_cli_project_state_metric_entry_failed",
                format!(
                    "Could not inspect an entry in `{}`: {error}",
                    path.display()
                ),
            )
        })?;
        let metadata = entry.metadata().map_err(|error| {
            io_error(
                "xero_cli_project_state_metric_metadata_failed",
                format!("Could not stat `{}`: {error}", entry.path().display()),
            )
        })?;
        if metadata.is_dir() {
            let (nested_files, nested_bytes) = directory_metrics(&entry.path())?;
            file_count = file_count.saturating_add(nested_files);
            byte_count = byte_count.saturating_add(nested_bytes);
        } else {
            file_count = file_count.saturating_add(1);
            byte_count = byte_count.saturating_add(metadata.len());
        }
    }
    Ok((file_count, byte_count))
}

fn remove_file_if_exists(path: &Path) -> Result<(), CliError> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(error) => Err(io_error(
            "xero_cli_project_state_remove_failed",
            format!("Could not remove `{}`: {error}", path.display()),
        )),
    }
}

fn remove_dir_if_present(path: &Path) -> Result<bool, CliError> {
    match fs::remove_dir_all(path) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(false),
        Err(error) => Err(io_error(
            "xero_cli_wipe_remove_failed",
            format!("Could not remove `{}`: {error}", path.display()),
        )),
    }
}

fn remove_dir_contents_if_present(path: &Path) -> Result<bool, CliError> {
    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(false),
        Err(error) => {
            return Err(io_error(
                "xero_cli_wipe_read_failed",
                format!("Could not enumerate `{}`: {error}", path.display()),
            ));
        }
    };
    for entry in entries {
        let entry = entry.map_err(|error| {
            io_error(
                "xero_cli_wipe_read_failed",
                format!("Could not read entry in `{}`: {error}", path.display()),
            )
        })?;
        let path = entry.path();
        if path.is_dir() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        }
        .map_err(|error| {
            io_error(
                "xero_cli_wipe_remove_failed",
                format!("Could not remove `{}`: {error}", path.display()),
            )
        })?;
    }
    Ok(true)
}

fn read_json_if_present(path: &Path) -> Option<JsonValue> {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
}

fn project_backups_root(globals: &GlobalOptions, project_id: &str) -> PathBuf {
    workspace_project_database_path(globals, project_id)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| cli_app_data_root(globals).join("projects").join(project_id))
        .join("backups")
}

fn backup_dir(globals: &GlobalOptions, project_id: &str, backup_id: &str) -> PathBuf {
    project_backups_root(globals, project_id).join(backup_id)
}

fn lance_dir_for_database(database_path: &Path) -> PathBuf {
    database_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lance")
}

fn backup_location(backup_id: &str) -> String {
    format!("app-data/backups/{backup_id}")
}

fn manifest_location_for(backup_id: &str) -> String {
    format!("app-data/backups/{backup_id}/manifest.json")
}

fn next_available_backup_dir(base: &Path) -> PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }
    let parent = base.parent().unwrap_or_else(|| Path::new("."));
    let name = base
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("backup");
    for attempt in 1u32.. {
        let candidate = parent.join(format!("{name}-{attempt}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("unbounded loop returns")
}

fn take_positional(args: &mut Vec<String>) -> Option<String> {
    if args.first().is_some_and(|arg| !arg.starts_with('-')) {
        Some(args.remove(0))
    } else {
        None
    }
}

fn validate_project_id(value: &str) -> Result<(), CliError> {
    if value.trim().is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(CliError::usage(
            "Project id must be a non-empty app-data-safe identifier.",
        ));
    }
    Ok(())
}

fn validate_backup_id(value: &str) -> Result<(), CliError> {
    if value.trim().is_empty()
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(CliError::usage(
            "Backup id must contain only letters, numbers, dots, dashes, or underscores.",
        ));
    }
    Ok(())
}

fn sanitize_backup_id(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn reject_project_state_unknown_options(args: &[String]) -> Result<(), CliError> {
    if let Some(option) = args.iter().find(|arg| arg.starts_with('-')) {
        return Err(CliError::usage(format!("Unknown option `{option}`.")));
    }
    if let Some(extra) = args.first() {
        return Err(CliError::usage(format!("Unexpected argument `{extra}`.")));
    }
    Ok(())
}

fn io_error(code: &'static str, message: String) -> CliError {
    CliError::system_fault(code, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn project_state_backup_restore_round_trips_app_data_state() {
        let state_dir = temp_dir("project-state-round-trip");
        let repo_dir = temp_dir("project-state-repo");
        seed_registered_project(&state_dir, "project-state", &repo_dir);
        let database_path = state_dir
            .join("projects")
            .join("project-state")
            .join("state.db");
        {
            let connection = Connection::open(&database_path).expect("state db");
            connection
                .execute_batch(
                    "CREATE TABLE notes(value TEXT); INSERT INTO notes(value) VALUES ('before');",
                )
                .expect("seed notes");
        }

        let backup = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project-state",
            "backup",
            "--project-id",
            "project-state",
            "--backup-id",
            "backup-test",
        ])
        .expect("backup");
        assert_eq!(
            backup.json["schema"],
            json!("xero.project_state_backup_command.v1")
        );

        {
            let connection = Connection::open(&database_path).expect("state db");
            connection
                .execute("UPDATE notes SET value = 'after'", [])
                .expect("mutate");
        }

        let restore = crate::run_with_args([
            "xero",
            "--json",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "project-state",
            "restore",
            "backup-test",
            "--project-id",
            "project-state",
            "--yes",
        ])
        .expect("restore");
        assert_eq!(
            restore.json["schema"],
            json!("xero.project_state_restore_command.v1")
        );

        let connection = Connection::open(&database_path).expect("state db");
        let value: String = connection
            .query_row("SELECT value FROM notes", [], |row| row.get(0))
            .expect("note");
        assert_eq!(value, "before");
    }

    #[test]
    fn wipe_project_requires_matching_confirmation() {
        let state_dir = temp_dir("project-state-wipe-gate");
        let error = crate::run_with_args([
            "xero",
            "--state-dir",
            state_dir.to_str().expect("state dir"),
            "wipe",
            "project",
            "project-state",
            "--yes",
            "--confirm",
            "wrong-project",
        ])
        .expect_err("confirmation should gate wipe");
        assert_eq!(error.code, "xero_cli_usage");
    }

    fn seed_registered_project(state_dir: &Path, project_id: &str, repo_root: &Path) {
        fs::create_dir_all(repo_root).expect("repo dir");
        fs::create_dir_all(state_dir).expect("state dir");
        let global_database = state_dir.join("xero.db");
        let connection = Connection::open(global_database).expect("global db");
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    start_targets TEXT NOT NULL DEFAULT '[]',
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS repositories (
                    id TEXT PRIMARY KEY,
                    project_id TEXT NOT NULL,
                    root_path TEXT NOT NULL,
                    display_name TEXT NOT NULL,
                    branch TEXT,
                    head_sha TEXT,
                    updated_at TEXT NOT NULL
                );
                "#,
            )
            .expect("global schema");
        connection
            .execute(
                "INSERT INTO projects (id, name, updated_at) VALUES (?1, ?2, ?3)",
                params![project_id, "Project", "2026-05-15T00:00:00Z"],
            )
            .expect("project row");
        connection
            .execute(
                "INSERT INTO repositories (id, project_id, root_path, display_name, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    format!("repo-{project_id}"),
                    project_id,
                    repo_root.to_string_lossy(),
                    "Project",
                    "2026-05-15T00:00:00Z"
                ],
            )
            .expect("repo row");

        let project_dir = state_dir.join("projects").join(project_id);
        fs::create_dir_all(&project_dir).expect("project dir");
        let project_database = project_dir.join("state.db");
        let connection = Connection::open(project_database).expect("project db");
        connection
            .execute_batch(
                r#"
                PRAGMA journal_mode = WAL;
                CREATE TABLE IF NOT EXISTS projects (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                );
                CREATE TABLE IF NOT EXISTS agent_sessions (
                    project_id TEXT NOT NULL,
                    agent_session_id TEXT NOT NULL,
                    title TEXT NOT NULL,
                    summary TEXT NOT NULL DEFAULT '',
                    status TEXT NOT NULL,
                    selected INTEGER NOT NULL DEFAULT 0,
                    remote_visible INTEGER NOT NULL DEFAULT 0,
                    created_at TEXT NOT NULL DEFAULT '2026-05-15T00:00:00Z',
                    updated_at TEXT NOT NULL,
                    archived_at TEXT,
                    last_run_id TEXT,
                    last_provider_id TEXT,
                    PRIMARY KEY (project_id, agent_session_id)
                );
                "#,
            )
            .expect("project schema");
        connection
            .execute(
                "INSERT INTO projects (id, name, updated_at) VALUES (?1, ?2, ?3)",
                params![project_id, "Project", "2026-05-15T00:00:00Z"],
            )
            .expect("project state row");
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("xero-cli-{prefix}-{nonce}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }
}
