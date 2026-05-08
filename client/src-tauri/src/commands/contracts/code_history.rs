use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeHistoryOperationModeDto {
    SelectiveUndo,
    SessionRollback,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeHistoryOperationStatusDto {
    Pending,
    Planning,
    Conflicted,
    Applying,
    Completed,
    Failed,
    RepairNeeded,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeHistoryTargetKindDto {
    ChangeGroup,
    FileChange,
    Hunks,
    SessionBoundary,
    RunBoundary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeHistoryConflictKindDto {
    TextOverlap,
    FileMissing,
    FileExists,
    ContentMismatch,
    MetadataMismatch,
    UnsupportedOperation,
    StaleWorkspace,
    StorageError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeWorkspaceHeadDto {
    pub project_id: String,
    pub head_id: Option<String>,
    pub tree_id: Option<String>,
    pub workspace_epoch: u64,
    pub latest_history_operation_id: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodePatchTextHunkDto {
    pub hunk_id: String,
    pub patch_file_id: Option<String>,
    pub file_path: String,
    pub hunk_index: u32,
    pub base_start_line: u32,
    pub base_line_count: u32,
    pub result_start_line: u32,
    pub result_line_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodePatchAvailabilityDto {
    pub project_id: String,
    pub target_change_group_id: String,
    pub available: bool,
    pub affected_paths: Vec<String>,
    pub file_change_count: u32,
    pub text_hunk_count: u32,
    #[serde(default)]
    pub text_hunks: Vec<CodePatchTextHunkDto>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectiveUndoTargetDto {
    pub target_kind: CodeHistoryTargetKindDto,
    pub target_id: String,
    pub change_group_id: Option<String>,
    pub file_path: Option<String>,
    #[serde(default)]
    pub hunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRollbackTargetDto {
    pub target_kind: CodeHistoryTargetKindDto,
    pub target_id: String,
    pub agent_session_id: String,
    pub boundary_id: String,
    pub run_id: Option<String>,
    pub change_group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeHistoryOperationTargetDto {
    pub target_kind: CodeHistoryTargetKindDto,
    pub target_id: String,
    #[serde(default)]
    pub hunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeHistoryConflictDto {
    pub operation_id: String,
    pub target_id: String,
    pub path: String,
    pub kind: CodeHistoryConflictKindDto,
    pub message: String,
    pub base_hash: Option<String>,
    pub selected_hash: Option<String>,
    pub current_hash: Option<String>,
    #[serde(default)]
    pub hunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeHistoryOperationDto {
    pub project_id: String,
    pub operation_id: String,
    pub mode: CodeHistoryOperationModeDto,
    pub status: CodeHistoryOperationStatusDto,
    pub target: CodeHistoryOperationTargetDto,
    pub affected_paths: Vec<String>,
    pub conflicts: Vec<CodeHistoryConflictDto>,
    pub workspace_head: Option<CodeWorkspaceHeadDto>,
    pub patch_availability: Option<CodePatchAvailabilityDto>,
    pub result_commit_id: Option<String>,
    pub result_change_group_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectiveUndoRequestDto {
    pub project_id: String,
    pub operation_id: String,
    pub target: SelectiveUndoTargetDto,
    pub expected_workspace_epoch: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectiveUndoResponseDto {
    pub operation: CodeHistoryOperationDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRollbackRequestDto {
    pub project_id: String,
    pub operation_id: String,
    pub target: SessionRollbackTargetDto,
    pub expected_workspace_epoch: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionRollbackResponseDto {
    pub operation: CodeHistoryOperationDto,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeHistoryOperationStatusRequestDto {
    pub project_id: String,
    pub operation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeHistoryOperationStatusResponseDto {
    pub operation: CodeHistoryOperationDto,
}

pub fn validate_selective_undo_request_contract(
    request: &SelectiveUndoRequestDto,
) -> Result<(), String> {
    require_non_empty(&request.project_id, "projectId")?;
    require_non_empty(&request.operation_id, "operationId")?;
    validate_selective_undo_target(&request.target)
}

pub fn validate_session_rollback_request_contract(
    request: &SessionRollbackRequestDto,
) -> Result<(), String> {
    require_non_empty(&request.project_id, "projectId")?;
    require_non_empty(&request.operation_id, "operationId")?;
    validate_session_rollback_target(&request.target)
}

pub fn validate_code_history_operation_contract(
    operation: &CodeHistoryOperationDto,
) -> Result<(), String> {
    require_non_empty(&operation.project_id, "projectId")?;
    require_non_empty(&operation.operation_id, "operationId")?;
    validate_target_summary(&operation.target)?;
    require_non_empty_items(&operation.affected_paths, "affectedPaths")?;
    require_non_empty(&operation.created_at, "createdAt")?;
    require_non_empty(&operation.updated_at, "updatedAt")?;

    if operation.status == CodeHistoryOperationStatusDto::Conflicted
        && operation.conflicts.is_empty()
    {
        return Err("conflicted operations must include conflict records".into());
    }

    for conflict in &operation.conflicts {
        validate_conflict(operation, conflict)?;
    }

    if let Some(workspace_head) = &operation.workspace_head {
        validate_workspace_head(workspace_head)?;
    }

    if let Some(availability) = &operation.patch_availability {
        validate_patch_availability(availability)?;
    }

    Ok(())
}

pub fn validate_selective_undo_response_contract(
    response: &SelectiveUndoResponseDto,
) -> Result<(), String> {
    validate_code_history_operation_contract(&response.operation)?;
    if response.operation.mode != CodeHistoryOperationModeDto::SelectiveUndo {
        return Err("selective undo responses must carry selective_undo operations".into());
    }
    Ok(())
}

pub fn validate_session_rollback_response_contract(
    response: &SessionRollbackResponseDto,
) -> Result<(), String> {
    validate_code_history_operation_contract(&response.operation)?;
    if response.operation.mode != CodeHistoryOperationModeDto::SessionRollback {
        return Err("session rollback responses must carry session_rollback operations".into());
    }
    Ok(())
}

pub fn validate_code_history_operation_status_request_contract(
    request: &CodeHistoryOperationStatusRequestDto,
) -> Result<(), String> {
    require_non_empty(&request.project_id, "projectId")?;
    require_non_empty(&request.operation_id, "operationId")
}

pub fn validate_code_history_operation_status_response_contract(
    response: &CodeHistoryOperationStatusResponseDto,
) -> Result<(), String> {
    validate_code_history_operation_contract(&response.operation)
}

fn validate_selective_undo_target(target: &SelectiveUndoTargetDto) -> Result<(), String> {
    require_non_empty(&target.target_id, "target.targetId")?;
    match target.target_kind {
        CodeHistoryTargetKindDto::ChangeGroup => {
            require_optional_non_empty(&target.change_group_id, "target.changeGroupId")
        }
        CodeHistoryTargetKindDto::FileChange => {
            require_optional_non_empty(&target.change_group_id, "target.changeGroupId")?;
            require_optional_non_empty(&target.file_path, "target.filePath")
        }
        CodeHistoryTargetKindDto::Hunks => {
            require_optional_non_empty(&target.change_group_id, "target.changeGroupId")?;
            require_optional_non_empty(&target.file_path, "target.filePath")?;
            require_non_empty_items(&target.hunk_ids, "target.hunkIds")
        }
        CodeHistoryTargetKindDto::SessionBoundary | CodeHistoryTargetKindDto::RunBoundary => Err(
            "selective undo targets must select a change group, file change, or hunk set".into(),
        ),
    }
}

fn validate_session_rollback_target(target: &SessionRollbackTargetDto) -> Result<(), String> {
    require_non_empty(&target.target_id, "target.targetId")?;
    require_non_empty(&target.agent_session_id, "target.agentSessionId")?;
    require_non_empty(&target.boundary_id, "target.boundaryId")?;
    match target.target_kind {
        CodeHistoryTargetKindDto::SessionBoundary => Ok(()),
        CodeHistoryTargetKindDto::RunBoundary => {
            require_optional_non_empty(&target.run_id, "target.runId")
        }
        CodeHistoryTargetKindDto::ChangeGroup
        | CodeHistoryTargetKindDto::FileChange
        | CodeHistoryTargetKindDto::Hunks => {
            Err("session rollback targets must select a session or run boundary".into())
        }
    }
}

fn validate_target_summary(target: &CodeHistoryOperationTargetDto) -> Result<(), String> {
    require_non_empty(&target.target_id, "target.targetId")?;
    validate_non_empty_items_when_present(&target.hunk_ids, "target.hunkIds")?;
    if target.target_kind == CodeHistoryTargetKindDto::Hunks && target.hunk_ids.is_empty() {
        return Err("hunk operation targets must record selected hunk ids".into());
    }
    Ok(())
}

fn validate_conflict(
    operation: &CodeHistoryOperationDto,
    conflict: &CodeHistoryConflictDto,
) -> Result<(), String> {
    require_non_empty(&conflict.operation_id, "conflicts[].operationId")?;
    require_non_empty(&conflict.target_id, "conflicts[].targetId")?;
    require_non_empty(&conflict.path, "conflicts[].path")?;
    require_non_empty(&conflict.message, "conflicts[].message")?;
    validate_non_empty_items_when_present(&conflict.hunk_ids, "conflicts[].hunkIds")?;

    if conflict.operation_id != operation.operation_id {
        return Err("conflict operation ids must match the enclosing operation".into());
    }

    if conflict.target_id != operation.target.target_id {
        return Err("conflict target ids must match the enclosing operation target".into());
    }

    if !operation
        .affected_paths
        .iter()
        .any(|path| path == &conflict.path)
    {
        return Err("conflict paths must be listed in affectedPaths".into());
    }

    Ok(())
}

fn validate_workspace_head(head: &CodeWorkspaceHeadDto) -> Result<(), String> {
    require_non_empty(&head.project_id, "workspaceHead.projectId")?;
    validate_optional_non_empty(&head.head_id, "workspaceHead.headId")?;
    validate_optional_non_empty(&head.tree_id, "workspaceHead.treeId")?;
    validate_optional_non_empty(
        &head.latest_history_operation_id,
        "workspaceHead.latestHistoryOperationId",
    )?;
    require_non_empty(&head.updated_at, "workspaceHead.updatedAt")
}

fn validate_patch_availability(availability: &CodePatchAvailabilityDto) -> Result<(), String> {
    require_non_empty(&availability.project_id, "patchAvailability.projectId")?;
    require_non_empty(
        &availability.target_change_group_id,
        "patchAvailability.targetChangeGroupId",
    )?;

    if availability.available {
        require_non_empty_items(
            &availability.affected_paths,
            "patchAvailability.affectedPaths",
        )?;
    } else {
        require_optional_non_empty(
            &availability.unavailable_reason,
            "patchAvailability.unavailableReason",
        )?;
    }

    for hunk in &availability.text_hunks {
        require_non_empty(&hunk.hunk_id, "patchAvailability.textHunks[].hunkId")?;
        validate_optional_non_empty(
            &hunk.patch_file_id,
            "patchAvailability.textHunks[].patchFileId",
        )?;
        require_non_empty(&hunk.file_path, "patchAvailability.textHunks[].filePath")?;
    }

    Ok(())
}

fn require_non_empty(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{field} is required"));
    }
    Ok(())
}

fn require_optional_non_empty(value: &Option<String>, field: &str) -> Result<(), String> {
    match value.as_deref() {
        Some(value) => require_non_empty(value, field),
        None => Err(format!("{field} is required")),
    }
}

fn validate_optional_non_empty(value: &Option<String>, field: &str) -> Result<(), String> {
    if let Some(value) = value.as_deref() {
        require_non_empty(value, field)?;
    }
    Ok(())
}

fn require_non_empty_items(values: &[String], field: &str) -> Result<(), String> {
    if values.is_empty() {
        return Err(format!("{field} is required"));
    }

    validate_non_empty_items_when_present(values, field)
}

fn validate_non_empty_items_when_present(values: &[String], field: &str) -> Result<(), String> {
    for value in values {
        require_non_empty(value, field)?;
    }

    Ok(())
}
