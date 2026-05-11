use std::{fs, io::ErrorKind, path::Path};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        runtime_support::resolve_project_root, validate_non_empty, CommandError, CommandResult,
    },
    db::{project_app_data_dir_for_repo, project_store},
    state::DesktopState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateProjectStateBackupRequestDto {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub backup_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RestoreProjectStateBackupRequestDto {
    pub project_id: String,
    pub backup_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepairProjectStateRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ListProjectStateBackupsRequestDto {
    pub project_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStateBackupListingEntryDto {
    pub backup_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<u64>,
    pub manifest_present: bool,
    pub pre_restore: bool,
    pub backup_location: String,
    pub manifest_location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ListProjectStateBackupsResponseDto {
    pub schema: String,
    pub project_id: String,
    pub storage_scope: String,
    pub backups: Vec<ProjectStateBackupListingEntryDto>,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadProjectUiStateRequestDto {
    pub project_id: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteProjectUiStateRequestDto {
    pub project_id: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUiStateResponseDto {
    pub schema: String,
    pub project_id: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
    pub storage_scope: String,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReadAppUiStateRequestDto {
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WriteAppUiStateRequestDto {
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AppUiStateResponseDto {
    pub schema: String,
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<JsonValue>,
    pub storage_scope: String,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStateBackupResponseDto {
    pub schema: String,
    pub project_id: String,
    pub backup_id: String,
    pub created_at: String,
    pub file_count: usize,
    pub byte_count: u64,
    pub storage_scope: String,
    pub backup_location: String,
    pub manifest_location: String,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStateRestoreResponseDto {
    pub schema: String,
    pub project_id: String,
    pub backup_id: String,
    pub restored_at: String,
    pub pre_restore_backup_id: String,
    pub storage_scope: String,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStateRepairResponseDto {
    pub schema: String,
    pub project_id: String,
    pub checked_at: String,
    pub sqlite_checkpointed: bool,
    pub outbox_inspected_count: usize,
    pub outbox_reconciled_count: usize,
    pub outbox_failed_count: usize,
    pub handoff_inspected_count: usize,
    pub handoff_repaired_count: usize,
    pub handoff_failed_count: usize,
    pub project_record_health_status: String,
    pub agent_memory_health_status: String,
    pub diagnostics: Vec<ProjectStateRepairDiagnosticDto>,
    pub storage_scope: String,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStateRepairDiagnosticDto {
    pub code: String,
    pub message: String,
    pub severity: String,
}

#[tauri::command]
pub fn read_project_ui_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ReadProjectUiStateRequestDto,
) -> CommandResult<ProjectUiStateResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let key = validate_project_ui_state_key(&request.key)?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let value = read_project_ui_state_value(&repo_root, &key)?;
    Ok(ProjectUiStateResponseDto {
        schema: "xero.project_ui_state.v1".into(),
        project_id: request.project_id,
        key,
        value,
        storage_scope: "os_app_data".into(),
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn write_project_ui_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WriteProjectUiStateRequestDto,
) -> CommandResult<ProjectUiStateResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let key = validate_project_ui_state_key(&request.key)?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    write_project_ui_state_value(&repo_root, &key, request.value.as_ref())?;
    Ok(ProjectUiStateResponseDto {
        schema: "xero.project_ui_state.v1".into(),
        project_id: request.project_id,
        key,
        value: request.value,
        storage_scope: "os_app_data".into(),
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn read_app_ui_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ReadAppUiStateRequestDto,
) -> CommandResult<AppUiStateResponseDto> {
    let key = validate_ui_state_key(&request.key)?;
    let app_data_dir = state.inner().app_data_dir(&app)?;
    let value = read_ui_state_value(&app_data_dir.join("ui-state"), &key, "app")?;
    Ok(AppUiStateResponseDto {
        schema: "xero.app_ui_state.v1".into(),
        key,
        value,
        storage_scope: "os_app_data".into(),
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn write_app_ui_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: WriteAppUiStateRequestDto,
) -> CommandResult<AppUiStateResponseDto> {
    let key = validate_ui_state_key(&request.key)?;
    let app_data_dir = state.inner().app_data_dir(&app)?;
    write_ui_state_value(
        &app_data_dir.join("ui-state"),
        &key,
        request.value.as_ref(),
        "app",
    )?;
    Ok(AppUiStateResponseDto {
        schema: "xero.app_ui_state.v1".into(),
        key,
        value: request.value,
        storage_scope: "os_app_data".into(),
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn create_project_state_backup<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: CreateProjectStateBackupRequestDto,
) -> CommandResult<ProjectStateBackupResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let created_at = now_timestamp();
    let backup_id = match request.backup_id {
        Some(backup_id) => validate_backup_id(&backup_id)?,
        None => format!("backup-{}", sanitize_backup_id(&created_at)),
    };
    let backup = project_store::create_project_state_backup(&repo_root, &backup_id, &created_at)?;
    Ok(ProjectStateBackupResponseDto {
        schema: "xero.project_state_backup_command.v1".into(),
        project_id: request.project_id,
        backup_id: backup.backup_id.clone(),
        created_at: backup.created_at,
        file_count: backup.file_count,
        byte_count: backup.byte_count,
        storage_scope: "os_app_data".into(),
        backup_location: format!("app-data/backups/{}", backup.backup_id),
        manifest_location: format!("app-data/backups/{}/manifest.json", backup.backup_id),
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn restore_project_state_backup<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RestoreProjectStateBackupRequestDto,
) -> CommandResult<ProjectStateRestoreResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let backup_id = validate_backup_id(&request.backup_id)?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let restored_at = now_timestamp();
    let backup_dir = project_app_data_dir_for_repo(&repo_root)
        .join("backups")
        .join(&backup_id);
    let restore =
        project_store::restore_project_state_backup(&repo_root, &backup_dir, &restored_at)?;
    Ok(ProjectStateRestoreResponseDto {
        schema: "xero.project_state_restore_command.v1".into(),
        project_id: request.project_id,
        backup_id,
        restored_at: restore.restored_at,
        pre_restore_backup_id: restore
            .pre_restore_backup_dir
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("pre-restore-backup")
            .to_string(),
        storage_scope: "os_app_data".into(),
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn list_project_state_backups<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ListProjectStateBackupsRequestDto,
) -> CommandResult<ListProjectStateBackupsResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let listing = project_store::list_project_state_backups(&repo_root)?;
    let backups = listing
        .backups
        .into_iter()
        .map(|entry| ProjectStateBackupListingEntryDto {
            backup_location: format!("app-data/backups/{}", entry.backup_id),
            manifest_location: format!("app-data/backups/{}/manifest.json", entry.backup_id),
            backup_id: entry.backup_id,
            created_at: entry.created_at,
            file_count: entry.file_count,
            byte_count: entry.byte_count,
            manifest_present: entry.manifest_present,
            pre_restore: entry.pre_restore,
        })
        .collect();
    Ok(ListProjectStateBackupsResponseDto {
        schema: "xero.project_state_backup_list_command.v1".into(),
        project_id: request.project_id,
        storage_scope: "os_app_data".into(),
        backups,
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn repair_project_state<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: RepairProjectStateRequestDto,
) -> CommandResult<ProjectStateRepairResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    let checked_at = now_timestamp();
    let repair = project_store::repair_project_state(&repo_root, &request.project_id, &checked_at)?;
    Ok(ProjectStateRepairResponseDto {
        schema: "xero.project_state_repair_command.v1".into(),
        project_id: repair.project_id,
        checked_at: repair.checked_at,
        sqlite_checkpointed: repair.sqlite_checkpointed,
        outbox_inspected_count: repair.outbox_inspected_count,
        outbox_reconciled_count: repair.outbox_reconciled_count,
        outbox_failed_count: repair.outbox_failed_count,
        handoff_inspected_count: repair.handoff_inspected_count,
        handoff_repaired_count: repair.handoff_repaired_count,
        handoff_failed_count: repair.handoff_failed_count,
        project_record_health_status: repair.project_record_health_status,
        agent_memory_health_status: repair.agent_memory_health_status,
        diagnostics: repair
            .diagnostics
            .into_iter()
            .map(|diagnostic| ProjectStateRepairDiagnosticDto {
                code: diagnostic.code,
                message: diagnostic.message,
                severity: diagnostic.severity,
            })
            .collect(),
        storage_scope: "os_app_data".into(),
        ui_deferred: true,
    })
}

fn validate_backup_id(value: &str) -> CommandResult<String> {
    validate_non_empty(value, "backupId")?;
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        Ok(value.to_string())
    } else {
        Err(CommandError::invalid_request("backupId"))
    }
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

fn validate_project_ui_state_key(value: &str) -> CommandResult<String> {
    validate_ui_state_key(value)
}

fn validate_ui_state_key(value: &str) -> CommandResult<String> {
    validate_non_empty(value, "key")?;
    let trimmed = value.trim();
    if trimmed.len() > 160
        || !trimmed
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | ':' | '@'))
    {
        return Err(CommandError::invalid_request("key"));
    }
    Ok(trimmed.to_string())
}

fn read_project_ui_state_value(repo_root: &Path, key: &str) -> CommandResult<Option<JsonValue>> {
    read_ui_state_value(
        &project_app_data_dir_for_repo(repo_root).join("ui-state"),
        key,
        "project",
    )
}

fn read_ui_state_value(
    state_dir: &Path,
    key: &str,
    storage_label: &str,
) -> CommandResult<Option<JsonValue>> {
    let path = ui_state_file_path(state_dir, key);
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CommandError::retryable(
                format!("{storage_label}_ui_state_read_failed"),
                format!(
                    "Xero could not read {storage_label} UI state `{key}` from app-data storage: {error}"
                ),
            ));
        }
    };

    let envelope: JsonValue = serde_json::from_str(&text).map_err(|error| {
        CommandError::retryable(
            format!("{storage_label}_ui_state_decode_failed"),
            format!("Xero could not decode {storage_label} UI state `{key}`: {error}"),
        )
    })?;
    if envelope.get("key").and_then(JsonValue::as_str) != Some(key) {
        return Err(CommandError::retryable(
            format!("{storage_label}_ui_state_key_mismatch"),
            format!(
                "Xero found app-data {storage_label} UI state under the wrong key while reading `{key}`."
            ),
        ));
    }
    Ok(envelope.get("value").cloned())
}

fn write_project_ui_state_value(
    repo_root: &Path,
    key: &str,
    value: Option<&JsonValue>,
) -> CommandResult<()> {
    write_ui_state_value(
        &project_app_data_dir_for_repo(repo_root).join("ui-state"),
        key,
        value,
        "project",
    )
}

fn write_ui_state_value(
    state_dir: &Path,
    key: &str,
    value: Option<&JsonValue>,
    storage_label: &str,
) -> CommandResult<()> {
    let path = ui_state_file_path(state_dir, key);
    if value.is_none() {
        match fs::remove_file(&path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => {
                return Err(CommandError::retryable(
                    format!("{storage_label}_ui_state_remove_failed"),
                    format!(
                        "Xero could not remove {storage_label} UI state `{key}` from app-data storage: {error}"
                    ),
                ));
            }
        }
    }

    let parent = path.parent().ok_or_else(|| {
        CommandError::system_fault(
            "project_ui_state_path_invalid",
            "Xero could not determine where to store project UI state.",
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CommandError::retryable(
            format!("{storage_label}_ui_state_dir_failed"),
            format!("Xero could not create {storage_label} UI state app-data storage: {error}"),
        )
    })?;
    let envelope = json!({
        "schema": format!("xero.{storage_label}_ui_state.v1"),
        "key": key,
        "value": value,
    });
    let bytes = serde_json::to_vec_pretty(&envelope).map_err(|error| {
        CommandError::system_fault(
            format!("{storage_label}_ui_state_encode_failed"),
            format!("Xero could not encode {storage_label} UI state `{key}`: {error}"),
        )
    })?;
    fs::write(&path, bytes).map_err(|error| {
        CommandError::retryable(
            format!("{storage_label}_ui_state_write_failed"),
            format!(
                "Xero could not write {storage_label} UI state `{key}` to app-data storage: {error}"
            ),
        )
    })
}

#[cfg(test)]
fn project_ui_state_file_path(repo_root: &Path, key: &str) -> std::path::PathBuf {
    ui_state_file_path(
        &project_app_data_dir_for_repo(repo_root).join("ui-state"),
        key,
    )
}

fn ui_state_file_path(state_dir: &Path, key: &str) -> std::path::PathBuf {
    state_dir.join(format!(
        "{}.json",
        hex_digest(Sha256::digest(key.as_bytes()).as_slice())
    ))
}

fn hex_digest(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_ui_state_round_trips_under_app_data_and_deletes() {
        let repo = tempfile::tempdir().expect("repo");
        let key = validate_project_ui_state_key("workflow.canvas.positions:ask:1").expect("key");
        let value = json!({
            "agent-header": { "x": 12, "y": -4 },
            "tool:read": { "x": 100, "y": 48 }
        });

        write_project_ui_state_value(repo.path(), &key, Some(&value)).expect("write state");
        assert_eq!(
            read_project_ui_state_value(repo.path(), &key).expect("read state"),
            Some(value)
        );
        assert!(project_ui_state_file_path(repo.path(), &key)
            .starts_with(project_app_data_dir_for_repo(repo.path())));

        write_project_ui_state_value(repo.path(), &key, None).expect("delete state");
        assert_eq!(
            read_project_ui_state_value(repo.path(), &key).expect("read deleted"),
            None
        );
    }

    #[test]
    fn app_ui_state_round_trips_under_app_data_and_deletes() {
        let app_data = tempfile::tempdir().expect("app data");
        let key = validate_ui_state_key("theme.active.v1").expect("key");
        let value = json!("midnight");
        let state_dir = app_data.path().join("ui-state");

        write_ui_state_value(&state_dir, &key, Some(&value), "app").expect("write state");
        assert_eq!(
            read_ui_state_value(&state_dir, &key, "app").expect("read state"),
            Some(value)
        );
        assert!(ui_state_file_path(&state_dir, &key).starts_with(app_data.path()));

        write_ui_state_value(&state_dir, &key, None, "app").expect("delete state");
        assert_eq!(
            read_ui_state_value(&state_dir, &key, "app").expect("read deleted"),
            None
        );
    }

    #[test]
    fn project_ui_state_rejects_path_like_keys() {
        assert!(validate_project_ui_state_key("../escape").is_err());
        assert!(validate_project_ui_state_key("workflow/canvas").is_err());
        assert!(validate_project_ui_state_key("workflow.canvas:agent_1@2").is_ok());
    }
}
