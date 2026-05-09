use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{runtime_support::resolve_project_root, validate_non_empty, CommandResult},
    db::project_store,
    state::DesktopState,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteProjectContextRecordRequestDto {
    pub project_id: String,
    pub record_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DeleteProjectContextRecordResponseDto {
    pub schema: String,
    pub project_id: String,
    pub record_id: String,
    pub retrieval_removed: bool,
    pub ui_deferred: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SupersedeProjectContextRecordRequestDto {
    pub project_id: String,
    pub superseded_record_id: String,
    pub superseding_record_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SupersedeProjectContextRecordResponseDto {
    pub schema: String,
    pub project_id: String,
    pub superseded_record_id: String,
    pub superseding_record_id: String,
    pub retrieval_changed: bool,
    pub ui_deferred: bool,
}

#[tauri::command]
pub fn delete_project_context_record<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: DeleteProjectContextRecordRequestDto,
) -> CommandResult<DeleteProjectContextRecordResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.record_id, "recordId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::delete_project_record(&repo_root, &request.project_id, &request.record_id)?;
    Ok(DeleteProjectContextRecordResponseDto {
        schema: "xero.project_context_record_delete_command.v1".into(),
        project_id: request.project_id,
        record_id: request.record_id,
        retrieval_removed: true,
        ui_deferred: true,
    })
}

#[tauri::command]
pub fn supersede_project_context_record<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SupersedeProjectContextRecordRequestDto,
) -> CommandResult<SupersedeProjectContextRecordResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.superseded_record_id, "supersededRecordId")?;
    validate_non_empty(&request.superseding_record_id, "supersedingRecordId")?;
    let repo_root = resolve_project_root(&app, state.inner(), &request.project_id)?;
    project_store::mark_project_record_superseded_by(
        &repo_root,
        &request.project_id,
        &request.superseded_record_id,
        &request.superseding_record_id,
        &now_timestamp(),
    )?;
    Ok(SupersedeProjectContextRecordResponseDto {
        schema: "xero.project_context_record_supersede_command.v1".into(),
        project_id: request.project_id,
        superseded_record_id: request.superseded_record_id,
        superseding_record_id: request.superseding_record_id,
        retrieval_changed: true,
        ui_deferred: true,
    })
}
