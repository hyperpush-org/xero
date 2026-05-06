use tauri::{AppHandle, Emitter, Runtime, State};

use crate::{
    commands::{
        validate_non_empty, ApplyCodeRollbackRequestDto, ApplyCodeRollbackResponseDto,
        CodeRollbackAffectedFileDto, CommandResult, RepositoryStatusChangedPayloadDto,
        REPOSITORY_STATUS_CHANGED_EVENT,
    },
    db::project_store::{self, AppliedCodeRollback},
    git::status,
    state::DesktopState,
};

#[tauri::command]
pub async fn apply_code_rollback<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: ApplyCodeRollbackRequestDto,
) -> CommandResult<ApplyCodeRollbackResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.target_change_group_id, "targetChangeGroupId")?;

    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id;
    let target_change_group_id = request.target_change_group_id;
    let operation_project_id = project_id.clone();
    let operation_registry_path = registry_path.clone();

    let applied = jobs
        .run_blocking_project_lane(
            project_id.clone(),
            "code_rollback",
            "code rollback",
            move || {
                let repository = status::resolve_project_repository(
                    &operation_project_id,
                    &operation_registry_path,
                )?;
                project_store::apply_code_rollback(
                    &repository.root_path,
                    &operation_project_id,
                    &target_change_group_id,
                )
            },
        )
        .await?;

    let repository_status = status::load_repository_status(&project_id, &registry_path)?;
    let payload = RepositoryStatusChangedPayloadDto {
        project_id: repository_status.repository.project_id.clone(),
        repository_id: repository_status.repository.id.clone(),
        status: repository_status.clone(),
    };
    let _ = app.emit(REPOSITORY_STATUS_CHANGED_EVENT, &payload);

    Ok(apply_code_rollback_response(applied, repository_status))
}

fn apply_code_rollback_response(
    applied: AppliedCodeRollback,
    repository_status: crate::commands::RepositoryStatusResponseDto,
) -> ApplyCodeRollbackResponseDto {
    ApplyCodeRollbackResponseDto {
        project_id: applied.project_id,
        agent_session_id: applied.agent_session_id,
        run_id: applied.run_id,
        operation_id: applied.operation_id,
        target_change_group_id: applied.target_change_group_id,
        target_snapshot_id: applied.target_snapshot_id,
        pre_rollback_snapshot_id: applied.pre_rollback_snapshot_id,
        result_change_group_id: applied.result_change_group_id,
        restored_paths: applied.restored_paths,
        removed_paths: applied.removed_paths,
        affected_files: applied
            .affected_files
            .into_iter()
            .map(|file| CodeRollbackAffectedFileDto {
                path_before: file.path_before,
                path_after: file.path_after,
                operation: file.operation.as_str().into(),
                before_hash: file.before_hash,
                after_hash: file.after_hash,
                explicitly_edited: file.explicitly_edited,
            })
            .collect(),
        repository_status,
    }
}
