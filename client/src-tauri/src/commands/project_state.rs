use serde::{Deserialize, Serialize};
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
