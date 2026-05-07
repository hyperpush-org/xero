use tauri::{AppHandle, Emitter, Runtime, State};

use crate::{
    auth::now_timestamp,
    commands::{
        validate_non_empty, CodeHistoryConflictDto, CodeHistoryConflictKindDto,
        CodeHistoryOperationDto, CodeHistoryOperationModeDto, CodeHistoryOperationStatusDto,
        CodeHistoryOperationTargetDto, CodeHistoryTargetKindDto, CodePatchAvailabilityDto,
        CodePatchTextHunkDto, CodeWorkspaceHeadDto, CommandError, CommandResult,
        RepositoryStatusChangedPayloadDto, SelectiveUndoRequestDto, SelectiveUndoResponseDto,
        SessionRollbackRequestDto, SessionRollbackResponseDto, REPOSITORY_STATUS_CHANGED_EVENT,
    },
    db::project_store::{
        self, AppliedCodeChangeGroupUndo, AppliedCodeFileUndo, AppliedCodeSessionRollback,
        ApplyCodeChangeGroupUndoRequest, ApplyCodeFileUndoRequest, ApplyCodeSessionRollbackRequest,
        CodeFileUndoApplyStatus, CodeFileUndoConflict, CodeFileUndoConflictKind,
        CodeSessionBoundaryTargetKind, ResolveCodeSessionBoundaryRequest,
    },
    git::status,
    state::DesktopState,
};

#[tauri::command]
pub async fn apply_selective_undo<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SelectiveUndoRequestDto,
) -> CommandResult<SelectiveUndoResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.operation_id, "operationId")?;

    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id.clone();
    let operation_project_id = project_id.clone();
    let operation_registry_path = registry_path.clone();
    let operation_id = request.operation_id.clone();
    let operation_target = request.target.clone();
    let response_target_kind = request.target.target_kind;
    let response_target_id = request.target.target_id.clone();
    let expected_workspace_epoch = request.expected_workspace_epoch;

    let applied = jobs
        .run_blocking_project_lane(
            project_id.clone(),
            "code_selective_undo",
            "code selective undo",
            move || {
                let repository = status::resolve_project_repository(
                    &operation_project_id,
                    &operation_registry_path,
                )?;
                let change_group_id = operation_target
                    .change_group_id
                    .clone()
                    .or_else(|| {
                        (operation_target.target_kind == CodeHistoryTargetKindDto::ChangeGroup)
                            .then(|| operation_target.target_id.clone())
                    })
                    .ok_or_else(|| CommandError::invalid_request("target.changeGroupId"))?;

                match operation_target.target_kind {
                    CodeHistoryTargetKindDto::FileChange => {
                        let target_file_path = operation_target
                            .file_path
                            .clone()
                            .ok_or_else(|| CommandError::invalid_request("target.filePath"))?;
                        project_store::apply_code_file_undo(
                            &repository.root_path,
                            ApplyCodeFileUndoRequest {
                                project_id: operation_project_id,
                                operation_id: Some(operation_id),
                                target_change_group_id: change_group_id,
                                target_patch_file_id: Some(operation_target.target_id.clone()),
                                target_file_path: Some(target_file_path),
                                target_hunk_ids: Vec::new(),
                                expected_workspace_epoch,
                            },
                        )
                        .map(AppliedSelectiveUndo::File)
                    }
                    CodeHistoryTargetKindDto::ChangeGroup => {
                        project_store::apply_code_change_group_undo(
                            &repository.root_path,
                            ApplyCodeChangeGroupUndoRequest {
                                project_id: operation_project_id,
                                operation_id: Some(operation_id),
                                target_change_group_id: change_group_id,
                                expected_workspace_epoch,
                            },
                        )
                        .map(AppliedSelectiveUndo::ChangeGroup)
                    }
                    CodeHistoryTargetKindDto::Hunks => {
                        let target_file_path = operation_target
                            .file_path
                            .clone()
                            .ok_or_else(|| CommandError::invalid_request("target.filePath"))?;
                        if operation_target.hunk_ids.is_empty() {
                            return Err(CommandError::invalid_request("target.hunkIds"));
                        }
                        project_store::apply_code_file_undo(
                            &repository.root_path,
                            ApplyCodeFileUndoRequest {
                                project_id: operation_project_id,
                                operation_id: Some(operation_id),
                                target_change_group_id: change_group_id,
                                target_patch_file_id: None,
                                target_file_path: Some(target_file_path),
                                target_hunk_ids: operation_target.hunk_ids.clone(),
                                expected_workspace_epoch,
                            },
                        )
                        .map(AppliedSelectiveUndo::File)
                    }
                    CodeHistoryTargetKindDto::SessionBoundary
                    | CodeHistoryTargetKindDto::RunBoundary => Err(CommandError::user_fixable(
                        "selective_undo_target_unsupported",
                        "Selective undo must target a change group, file change, or hunk set.",
                    )),
                }
            },
        )
        .await?;

    if applied.status() == CodeFileUndoApplyStatus::Completed {
        let repository_status = status::load_repository_status(&project_id, &registry_path)?;
        let payload = RepositoryStatusChangedPayloadDto {
            project_id: repository_status.repository.project_id.clone(),
            repository_id: repository_status.repository.id.clone(),
            status: repository_status,
        };
        let _ = app.emit(REPOSITORY_STATUS_CHANGED_EVENT, &payload);
    }

    Ok(SelectiveUndoResponseDto {
        operation: selective_undo_operation_response(
            response_target_kind,
            response_target_id,
            applied,
        ),
    })
}

#[tauri::command]
pub async fn apply_session_rollback<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopState>,
    request: SessionRollbackRequestDto,
) -> CommandResult<SessionRollbackResponseDto> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.operation_id, "operationId")?;

    let registry_path = state.global_db_path(&app)?;
    let jobs = state.backend_jobs().clone();
    let project_id = request.project_id.clone();
    let operation_project_id = project_id.clone();
    let operation_registry_path = registry_path.clone();
    let operation_id = request.operation_id.clone();
    let operation_target = request.target.clone();
    let response_target_kind = request.target.target_kind;
    let response_target_id = request.target.target_id.clone();
    let expected_workspace_epoch = request.expected_workspace_epoch;

    let applied = jobs
        .run_blocking_project_lane(
            project_id.clone(),
            "code_session_rollback",
            "code session rollback",
            move || {
                let repository = status::resolve_project_repository(
                    &operation_project_id,
                    &operation_registry_path,
                )?;
                let target_kind = session_rollback_boundary_kind(operation_target.target_kind)?;
                project_store::apply_code_session_rollback(
                    &repository.root_path,
                    ApplyCodeSessionRollbackRequest {
                        boundary: ResolveCodeSessionBoundaryRequest {
                            project_id: operation_project_id,
                            agent_session_id: operation_target.agent_session_id,
                            target_kind,
                            target_id: operation_target.target_id,
                            boundary_id: operation_target.boundary_id,
                            run_id: operation_target.run_id,
                            change_group_id: operation_target.change_group_id,
                        },
                        operation_id: Some(operation_id),
                        explicitly_selected_change_group_ids: Vec::new(),
                        expected_workspace_epoch,
                    },
                )
            },
        )
        .await?;

    if applied.status == CodeFileUndoApplyStatus::Completed {
        let repository_status = status::load_repository_status(&project_id, &registry_path)?;
        let payload = RepositoryStatusChangedPayloadDto {
            project_id: repository_status.repository.project_id.clone(),
            repository_id: repository_status.repository.id.clone(),
            status: repository_status,
        };
        let _ = app.emit(REPOSITORY_STATUS_CHANGED_EVENT, &payload);
    }

    Ok(SessionRollbackResponseDto {
        operation: session_rollback_operation_response(
            response_target_kind,
            response_target_id,
            applied,
        ),
    })
}

enum AppliedSelectiveUndo {
    File(AppliedCodeFileUndo),
    ChangeGroup(AppliedCodeChangeGroupUndo),
}

impl AppliedSelectiveUndo {
    fn status(&self) -> CodeFileUndoApplyStatus {
        match self {
            Self::File(applied) => applied.status,
            Self::ChangeGroup(applied) => applied.status,
        }
    }

    fn into_parts(self) -> SelectiveUndoOperationParts {
        match self {
            Self::File(applied) => SelectiveUndoOperationParts {
                project_id: applied.project_id,
                operation_id: applied.operation_id,
                status: applied.status,
                affected_paths: applied.affected_paths,
                conflicts: applied.conflicts,
                selected_hunk_ids: applied.selected_hunk_ids,
                workspace_head: applied.workspace_head,
                patch_availability: applied.patch_availability,
                result_commit_id: applied.result_commit_id,
                result_change_group_id: applied.result_change_group_id,
            },
            Self::ChangeGroup(applied) => SelectiveUndoOperationParts {
                project_id: applied.project_id,
                operation_id: applied.operation_id,
                status: applied.status,
                affected_paths: applied.affected_paths,
                conflicts: applied.conflicts,
                selected_hunk_ids: Vec::new(),
                workspace_head: applied.workspace_head,
                patch_availability: applied.patch_availability,
                result_commit_id: applied.result_commit_id,
                result_change_group_id: applied.result_change_group_id,
            },
        }
    }
}

struct SelectiveUndoOperationParts {
    project_id: String,
    operation_id: String,
    status: CodeFileUndoApplyStatus,
    affected_paths: Vec<String>,
    conflicts: Vec<CodeFileUndoConflict>,
    selected_hunk_ids: Vec<String>,
    workspace_head: Option<project_store::CodeWorkspaceHeadRecord>,
    patch_availability: Option<project_store::CodeChangeGroupHistoryMetadataRecord>,
    result_commit_id: Option<String>,
    result_change_group_id: Option<String>,
}

fn selective_undo_operation_response(
    target_kind: CodeHistoryTargetKindDto,
    target_id: String,
    applied: AppliedSelectiveUndo,
) -> CodeHistoryOperationDto {
    let now = now_timestamp();
    let applied = applied.into_parts();
    let target = CodeHistoryOperationTargetDto {
        target_kind,
        target_id,
        hunk_ids: applied.selected_hunk_ids.clone(),
    };
    let conflicts = applied
        .conflicts
        .iter()
        .map(|conflict| CodeHistoryConflictDto {
            operation_id: applied.operation_id.clone(),
            target_id: target.target_id.clone(),
            path: conflict.path.clone(),
            kind: undo_conflict_kind_dto(conflict.kind),
            message: conflict.message.clone(),
            base_hash: conflict.base_hash.clone(),
            selected_hash: conflict.selected_hash.clone(),
            current_hash: conflict.current_hash.clone(),
            hunk_ids: conflict.hunk_ids.clone(),
        })
        .collect();

    CodeHistoryOperationDto {
        project_id: applied.project_id,
        operation_id: applied.operation_id,
        mode: CodeHistoryOperationModeDto::SelectiveUndo,
        status: undo_status_dto(applied.status),
        target,
        affected_paths: applied.affected_paths,
        conflicts,
        workspace_head: applied.workspace_head.map(|head| CodeWorkspaceHeadDto {
            project_id: head.project_id,
            head_id: head.head_id,
            tree_id: head.tree_id,
            workspace_epoch: head.workspace_epoch,
            latest_history_operation_id: head.latest_history_operation_id,
            updated_at: head.updated_at,
        }),
        patch_availability: applied
            .patch_availability
            .map(|metadata| code_patch_availability_dto(metadata.patch_availability)),
        result_commit_id: applied.result_commit_id,
        result_change_group_id: applied.result_change_group_id,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn code_patch_availability_dto(
    availability: project_store::CodePatchAvailabilityRecord,
) -> CodePatchAvailabilityDto {
    CodePatchAvailabilityDto {
        project_id: availability.project_id,
        target_change_group_id: availability.target_change_group_id,
        available: availability.available,
        affected_paths: availability.affected_paths,
        file_change_count: availability.file_change_count,
        text_hunk_count: availability.text_hunk_count,
        text_hunks: availability
            .text_hunks
            .into_iter()
            .map(|hunk| CodePatchTextHunkDto {
                hunk_id: hunk.hunk_id,
                patch_file_id: hunk.patch_file_id,
                file_path: hunk.file_path,
                hunk_index: hunk.hunk_index,
                base_start_line: hunk.base_start_line,
                base_line_count: hunk.base_line_count,
                result_start_line: hunk.result_start_line,
                result_line_count: hunk.result_line_count,
            })
            .collect(),
        unavailable_reason: availability.unavailable_reason,
    }
}

fn session_rollback_operation_response(
    target_kind: CodeHistoryTargetKindDto,
    target_id: String,
    applied: AppliedCodeSessionRollback,
) -> CodeHistoryOperationDto {
    let now = now_timestamp();
    let target = CodeHistoryOperationTargetDto {
        target_kind,
        target_id,
        hunk_ids: Vec::new(),
    };
    let conflicts = applied
        .conflicts
        .iter()
        .map(|conflict| CodeHistoryConflictDto {
            operation_id: applied.operation_id.clone(),
            target_id: target.target_id.clone(),
            path: conflict.path.clone(),
            kind: undo_conflict_kind_dto(conflict.kind),
            message: conflict.message.clone(),
            base_hash: conflict.base_hash.clone(),
            selected_hash: conflict.selected_hash.clone(),
            current_hash: conflict.current_hash.clone(),
            hunk_ids: conflict.hunk_ids.clone(),
        })
        .collect();

    CodeHistoryOperationDto {
        project_id: applied.project_id,
        operation_id: applied.operation_id,
        mode: CodeHistoryOperationModeDto::SessionRollback,
        status: undo_status_dto(applied.status),
        target,
        affected_paths: applied.affected_paths,
        conflicts,
        workspace_head: applied.workspace_head.map(|head| CodeWorkspaceHeadDto {
            project_id: head.project_id,
            head_id: head.head_id,
            tree_id: head.tree_id,
            workspace_epoch: head.workspace_epoch,
            latest_history_operation_id: head.latest_history_operation_id,
            updated_at: head.updated_at,
        }),
        patch_availability: applied
            .patch_availability
            .map(|metadata| code_patch_availability_dto(metadata.patch_availability)),
        result_commit_id: applied.result_commit_id,
        result_change_group_id: applied.result_change_group_id,
        created_at: now.clone(),
        updated_at: now,
    }
}

fn session_rollback_boundary_kind(
    target_kind: CodeHistoryTargetKindDto,
) -> CommandResult<CodeSessionBoundaryTargetKind> {
    match target_kind {
        CodeHistoryTargetKindDto::SessionBoundary => {
            Ok(CodeSessionBoundaryTargetKind::SessionBoundary)
        }
        CodeHistoryTargetKindDto::RunBoundary => Ok(CodeSessionBoundaryTargetKind::RunBoundary),
        CodeHistoryTargetKindDto::ChangeGroup
        | CodeHistoryTargetKindDto::FileChange
        | CodeHistoryTargetKindDto::Hunks => Err(CommandError::user_fixable(
            "session_rollback_target_unsupported",
            "Session rollback must target a session or run boundary.",
        )),
    }
}

fn undo_status_dto(status: CodeFileUndoApplyStatus) -> CodeHistoryOperationStatusDto {
    match status {
        CodeFileUndoApplyStatus::Completed => CodeHistoryOperationStatusDto::Completed,
        CodeFileUndoApplyStatus::Conflicted => CodeHistoryOperationStatusDto::Conflicted,
    }
}

fn undo_conflict_kind_dto(kind: CodeFileUndoConflictKind) -> CodeHistoryConflictKindDto {
    match kind {
        CodeFileUndoConflictKind::TextOverlap => CodeHistoryConflictKindDto::TextOverlap,
        CodeFileUndoConflictKind::FileMissing => CodeHistoryConflictKindDto::FileMissing,
        CodeFileUndoConflictKind::FileExists => CodeHistoryConflictKindDto::FileExists,
        CodeFileUndoConflictKind::ContentMismatch => CodeHistoryConflictKindDto::ContentMismatch,
        CodeFileUndoConflictKind::MetadataMismatch => CodeHistoryConflictKindDto::MetadataMismatch,
        CodeFileUndoConflictKind::UnsupportedOperation => {
            CodeHistoryConflictKindDto::UnsupportedOperation
        }
        CodeFileUndoConflictKind::StaleWorkspace => CodeHistoryConflictKindDto::StaleWorkspace,
    }
}
