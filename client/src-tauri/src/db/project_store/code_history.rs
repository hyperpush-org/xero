use std::{collections::BTreeSet, path::Path};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::database_path_for_repo,
};

use super::{open_project_database, read_project_row};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeWorkspaceHeadRecord {
    pub project_id: String,
    pub head_id: Option<String>,
    pub tree_id: Option<String>,
    pub workspace_epoch: u64,
    pub latest_history_operation_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodePathEpochRecord {
    pub project_id: String,
    pub path: String,
    pub workspace_epoch: u64,
    pub commit_id: Option<String>,
    pub history_operation_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvanceCodeWorkspaceEpochRequest {
    pub project_id: String,
    pub head_id: Option<String>,
    pub tree_id: Option<String>,
    pub commit_id: Option<String>,
    pub latest_history_operation_id: Option<String>,
    pub affected_paths: Vec<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdvanceCodeWorkspaceEpochResult {
    pub workspace_head: CodeWorkspaceHeadRecord,
    pub path_epochs: Vec<CodePathEpochRecord>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeHistoryCommitKind {
    ChangeGroup,
    Undo,
    SessionRollback,
    RecoveredMutation,
    ImportedBaseline,
}

impl CodeHistoryCommitKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ChangeGroup => "change_group",
            Self::Undo => "undo",
            Self::SessionRollback => "session_rollback",
            Self::RecoveredMutation => "recovered_mutation",
            Self::ImportedBaseline => "imported_baseline",
        }
    }

    fn from_sql(value: &str) -> CommandResult<Self> {
        match value {
            "change_group" => Ok(Self::ChangeGroup),
            "undo" => Ok(Self::Undo),
            "session_rollback" => Ok(Self::SessionRollback),
            "recovered_mutation" => Ok(Self::RecoveredMutation),
            "imported_baseline" => Ok(Self::ImportedBaseline),
            _ => Err(decode_error(
                "commit_kind",
                format!("Unknown code history commit kind `{value}`."),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodePatchFileOperation {
    Create,
    Modify,
    Delete,
    Rename,
    ModeChange,
    SymlinkChange,
}

impl CodePatchFileOperation {
    fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
            Self::Rename => "rename",
            Self::ModeChange => "mode_change",
            Self::SymlinkChange => "symlink_change",
        }
    }

    fn from_sql(value: &str) -> CommandResult<Self> {
        match value {
            "create" => Ok(Self::Create),
            "modify" => Ok(Self::Modify),
            "delete" => Ok(Self::Delete),
            "rename" => Ok(Self::Rename),
            "mode_change" => Ok(Self::ModeChange),
            "symlink_change" => Ok(Self::SymlinkChange),
            _ => Err(decode_error(
                "operation",
                format!("Unknown code patch file operation `{value}`."),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodePatchMergePolicy {
    Text,
    Exact,
}

impl CodePatchMergePolicy {
    fn as_str(self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Exact => "exact",
        }
    }

    fn from_sql(value: &str) -> CommandResult<Self> {
        match value {
            "text" => Ok(Self::Text),
            "exact" => Ok(Self::Exact),
            _ => Err(decode_error(
                "merge_policy",
                format!("Unknown code patch merge policy `{value}`."),
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodePatchFileKind {
    File,
    Directory,
    Symlink,
}

impl CodePatchFileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
        }
    }

    fn from_sql(value: &str) -> CommandResult<Self> {
        match value {
            "file" => Ok(Self::File),
            "directory" => Ok(Self::Directory),
            "symlink" => Ok(Self::Symlink),
            _ => Err(decode_error(
                "file_kind",
                format!("Unknown code patch file kind `{value}`."),
            )),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePatchsetCommitInput {
    pub project_id: String,
    pub commit_id: String,
    pub parent_commit_id: Option<String>,
    pub tree_id: String,
    pub parent_tree_id: Option<String>,
    pub patchset_id: String,
    pub change_group_id: String,
    pub history_operation_id: Option<String>,
    pub agent_session_id: String,
    pub run_id: String,
    pub tool_call_id: Option<String>,
    pub runtime_event_id: Option<i64>,
    pub conversation_sequence: Option<i64>,
    pub commit_kind: CodeHistoryCommitKind,
    pub summary_label: String,
    pub workspace_epoch: u64,
    pub created_at: String,
    pub completed_at: String,
    pub files: Vec<CodePatchFileInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePatchFileInput {
    pub patch_file_id: String,
    pub path_before: Option<String>,
    pub path_after: Option<String>,
    pub operation: CodePatchFileOperation,
    pub merge_policy: CodePatchMergePolicy,
    pub before_file_kind: Option<CodePatchFileKind>,
    pub after_file_kind: Option<CodePatchFileKind>,
    pub base_hash: Option<String>,
    pub result_hash: Option<String>,
    pub base_blob_id: Option<String>,
    pub result_blob_id: Option<String>,
    pub base_size: Option<u64>,
    pub result_size: Option<u64>,
    pub base_mode: Option<u32>,
    pub result_mode: Option<u32>,
    pub base_symlink_target: Option<String>,
    pub result_symlink_target: Option<String>,
    pub hunks: Vec<CodePatchHunkInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePatchHunkInput {
    pub hunk_id: String,
    pub hunk_index: u32,
    pub base_start_line: u32,
    pub base_line_count: u32,
    pub result_start_line: u32,
    pub result_line_count: u32,
    pub removed_lines: Vec<String>,
    pub added_lines: Vec<String>,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePatchsetCommitRecord {
    pub commit: CodeCommitRecord,
    pub patchset: CodePatchsetRecord,
    pub files: Vec<CodePatchFileRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeCommitRecord {
    pub project_id: String,
    pub commit_id: String,
    pub parent_commit_id: Option<String>,
    pub tree_id: String,
    pub parent_tree_id: Option<String>,
    pub patchset_id: String,
    pub change_group_id: String,
    pub history_operation_id: Option<String>,
    pub agent_session_id: String,
    pub run_id: String,
    pub tool_call_id: Option<String>,
    pub runtime_event_id: Option<i64>,
    pub conversation_sequence: Option<i64>,
    pub commit_kind: CodeHistoryCommitKind,
    pub summary_label: String,
    pub workspace_epoch: u64,
    pub created_at: String,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePatchsetRecord {
    pub project_id: String,
    pub patchset_id: String,
    pub change_group_id: String,
    pub base_commit_id: Option<String>,
    pub base_tree_id: Option<String>,
    pub result_tree_id: String,
    pub patch_kind: CodeHistoryCommitKind,
    pub file_count: u32,
    pub text_hunk_count: u32,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePatchFileRecord {
    pub project_id: String,
    pub patchset_id: String,
    pub patch_file_id: String,
    pub file_index: u32,
    pub path_before: Option<String>,
    pub path_after: Option<String>,
    pub operation: CodePatchFileOperation,
    pub merge_policy: CodePatchMergePolicy,
    pub before_file_kind: Option<CodePatchFileKind>,
    pub after_file_kind: Option<CodePatchFileKind>,
    pub base_hash: Option<String>,
    pub result_hash: Option<String>,
    pub base_blob_id: Option<String>,
    pub result_blob_id: Option<String>,
    pub base_size: Option<u64>,
    pub result_size: Option<u64>,
    pub base_mode: Option<u32>,
    pub result_mode: Option<u32>,
    pub base_symlink_target: Option<String>,
    pub result_symlink_target: Option<String>,
    pub text_hunk_count: u32,
    pub created_at: String,
    pub hunks: Vec<CodePatchHunkRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodePatchHunkRecord {
    pub project_id: String,
    pub patch_file_id: String,
    pub hunk_id: String,
    pub hunk_index: u32,
    pub base_start_line: u32,
    pub base_line_count: u32,
    pub result_start_line: u32,
    pub result_line_count: u32,
    pub removed_lines: Vec<String>,
    pub added_lines: Vec<String>,
    pub context_before: Vec<String>,
    pub context_after: Vec<String>,
    pub created_at: String,
}

pub fn ensure_code_workspace_head(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<CodeWorkspaceHeadRecord> {
    validate_required_text(project_id, "projectId")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let now = now_timestamp();
    ensure_code_workspace_head_in_connection(&connection, project_id, &now, &database_path)?;
    read_code_workspace_head_from_connection(&connection, project_id, &database_path)?
        .ok_or_else(|| missing_project_head_error(project_id))
}

pub fn read_code_workspace_head(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Option<CodeWorkspaceHeadRecord>> {
    validate_required_text(project_id, "projectId")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_code_workspace_head_from_connection(&connection, project_id, &database_path)
}

pub fn advance_code_workspace_epoch(
    repo_root: &Path,
    request: &AdvanceCodeWorkspaceEpochRequest,
) -> CommandResult<AdvanceCodeWorkspaceEpochResult> {
    validate_advance_request(request)?;
    let affected_paths = normalize_affected_paths(&request.affected_paths)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_code_history_storage_error(
            &database_path,
            "code_workspace_epoch_transaction_failed",
            error,
        )
    })?;

    ensure_code_workspace_head_in_connection(
        &transaction,
        &request.project_id,
        &request.updated_at,
        &database_path,
    )?;
    let current_head = read_code_workspace_head_from_connection(
        &transaction,
        &request.project_id,
        &database_path,
    )?
    .ok_or_else(|| missing_project_head_error(&request.project_id))?;
    let next_epoch = current_head.workspace_epoch.checked_add(1).ok_or_else(|| {
        CommandError::system_fault(
            "code_workspace_epoch_overflow",
            format!(
                "Xero could not advance code workspace epoch for project `{}` because the current epoch is already at the maximum value.",
                request.project_id
            ),
        )
    })?;
    let next_epoch_sql = epoch_to_sql(next_epoch)?;

    transaction
        .execute(
            r#"
            UPDATE code_workspace_heads
            SET head_id = ?2,
                tree_id = ?3,
                workspace_epoch = ?4,
                latest_history_operation_id = ?5,
                updated_at = ?6
            WHERE project_id = ?1
            "#,
            params![
                request.project_id,
                request.head_id,
                request.tree_id,
                next_epoch_sql,
                request.latest_history_operation_id,
                request.updated_at,
            ],
        )
        .map_err(|error| {
            map_code_history_storage_error(
                &database_path,
                "code_workspace_head_advance_failed",
                error,
            )
        })?;

    for path in &affected_paths {
        transaction
            .execute(
                r#"
                INSERT INTO code_path_epochs (
                    project_id,
                    path,
                    workspace_epoch,
                    commit_id,
                    history_operation_id,
                    created_at,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)
                ON CONFLICT(project_id, path) DO UPDATE SET
                    workspace_epoch = excluded.workspace_epoch,
                    commit_id = excluded.commit_id,
                    history_operation_id = excluded.history_operation_id,
                    updated_at = excluded.updated_at
                "#,
                params![
                    request.project_id,
                    path,
                    next_epoch_sql,
                    request.commit_id,
                    request.latest_history_operation_id,
                    request.updated_at,
                ],
            )
            .map_err(|error| {
                map_code_history_storage_error(
                    &database_path,
                    "code_path_epoch_upsert_failed",
                    error,
                )
            })?;
    }

    let workspace_head = read_code_workspace_head_from_connection(
        &transaction,
        &request.project_id,
        &database_path,
    )?
    .ok_or_else(|| missing_project_head_error(&request.project_id))?;
    let path_epochs = read_code_path_epochs_from_connection(
        &transaction,
        &request.project_id,
        &affected_paths,
        &database_path,
    )?;

    transaction.commit().map_err(|error| {
        map_code_history_storage_error(&database_path, "code_workspace_epoch_commit_failed", error)
    })?;

    Ok(AdvanceCodeWorkspaceEpochResult {
        workspace_head,
        path_epochs,
    })
}

pub fn read_code_path_epoch(
    repo_root: &Path,
    project_id: &str,
    path: &str,
) -> CommandResult<Option<CodePathEpochRecord>> {
    validate_required_text(project_id, "projectId")?;
    validate_required_text(path, "path")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_code_path_epoch_from_connection(&connection, project_id, path, &database_path)
}

pub fn persist_code_patchset_commit(
    repo_root: &Path,
    input: &CodePatchsetCommitInput,
) -> CommandResult<CodePatchsetCommitRecord> {
    validate_patchset_commit_input(input)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let transaction = connection.unchecked_transaction().map_err(|error| {
        map_code_history_storage_error(&database_path, "code_commit_transaction_failed", error)
    })?;

    read_project_row(&transaction, &database_path, repo_root, &input.project_id)?;
    validate_completed_change_group_for_commit(&transaction, input, &database_path)?;
    insert_code_patchset(&transaction, input, &database_path)?;
    for (file_index, file) in input.files.iter().enumerate() {
        insert_code_patch_file(
            &transaction,
            input,
            file,
            usize_to_sql(file_index, "fileIndex")?,
            &database_path,
        )?;
        for hunk in &file.hunks {
            insert_code_patch_hunk(&transaction, input, file, hunk, &database_path)?;
        }
    }
    insert_code_commit(&transaction, input, &database_path)?;

    transaction.commit().map_err(|error| {
        map_code_history_storage_error(&database_path, "code_commit_persist_failed", error)
    })?;

    read_code_patchset_commit(repo_root, &input.project_id, &input.commit_id)?
        .ok_or_else(|| missing_commit_after_persist_error(&input.commit_id))
}

pub fn read_code_patchset_commit(
    repo_root: &Path,
    project_id: &str,
    commit_id: &str,
) -> CommandResult<Option<CodePatchsetCommitRecord>> {
    validate_required_text(project_id, "projectId")?;
    validate_required_text(commit_id, "commitId")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let Some(commit) =
        read_code_commit_from_connection(&connection, project_id, commit_id, &database_path)?
    else {
        return Ok(None);
    };
    let patchset = read_code_patchset_from_connection(
        &connection,
        project_id,
        &commit.patchset_id,
        &database_path,
    )?
    .ok_or_else(|| missing_patchset_for_commit_error(&commit))?;
    let files = read_code_patch_files_from_connection(
        &connection,
        project_id,
        &patchset.patchset_id,
        &database_path,
    )?;

    Ok(Some(CodePatchsetCommitRecord {
        commit,
        patchset,
        files,
    }))
}

#[derive(Debug)]
struct CompletedChangeGroupMetadata {
    agent_session_id: String,
    run_id: String,
}

#[derive(Debug)]
struct RawCodeCommitRow {
    project_id: String,
    commit_id: String,
    parent_commit_id: Option<String>,
    tree_id: String,
    parent_tree_id: Option<String>,
    patchset_id: String,
    change_group_id: String,
    history_operation_id: Option<String>,
    agent_session_id: String,
    run_id: String,
    tool_call_id: Option<String>,
    runtime_event_id: Option<i64>,
    conversation_sequence: Option<i64>,
    commit_kind: String,
    summary_label: String,
    workspace_epoch: i64,
    created_at: String,
    completed_at: String,
}

#[derive(Debug)]
struct RawCodePatchsetRow {
    project_id: String,
    patchset_id: String,
    change_group_id: String,
    base_commit_id: Option<String>,
    base_tree_id: Option<String>,
    result_tree_id: String,
    patch_kind: String,
    file_count: i64,
    text_hunk_count: i64,
    created_at: String,
}

#[derive(Debug)]
struct RawCodePatchFileRow {
    project_id: String,
    patchset_id: String,
    patch_file_id: String,
    file_index: i64,
    path_before: Option<String>,
    path_after: Option<String>,
    operation: String,
    merge_policy: String,
    before_file_kind: Option<String>,
    after_file_kind: Option<String>,
    base_hash: Option<String>,
    result_hash: Option<String>,
    base_blob_id: Option<String>,
    result_blob_id: Option<String>,
    base_size: Option<i64>,
    result_size: Option<i64>,
    base_mode: Option<i64>,
    result_mode: Option<i64>,
    base_symlink_target: Option<String>,
    result_symlink_target: Option<String>,
    text_hunk_count: i64,
    created_at: String,
}

#[derive(Debug)]
struct RawCodePatchHunkRow {
    project_id: String,
    patch_file_id: String,
    hunk_id: String,
    hunk_index: i64,
    base_start_line: i64,
    base_line_count: i64,
    result_start_line: i64,
    result_line_count: i64,
    removed_lines_json: String,
    added_lines_json: String,
    context_before_json: String,
    context_after_json: String,
    created_at: String,
}

fn validate_patchset_commit_input(input: &CodePatchsetCommitInput) -> CommandResult<()> {
    validate_required_text(&input.project_id, "projectId")?;
    validate_required_text(&input.commit_id, "commitId")?;
    validate_optional_text(input.parent_commit_id.as_deref(), "parentCommitId")?;
    validate_required_text(&input.tree_id, "treeId")?;
    validate_optional_text(input.parent_tree_id.as_deref(), "parentTreeId")?;
    validate_required_text(&input.patchset_id, "patchsetId")?;
    validate_required_text(&input.change_group_id, "changeGroupId")?;
    validate_optional_text(input.history_operation_id.as_deref(), "historyOperationId")?;
    validate_required_text(&input.agent_session_id, "agentSessionId")?;
    validate_required_text(&input.run_id, "runId")?;
    validate_optional_text(input.tool_call_id.as_deref(), "toolCallId")?;
    validate_runtime_event_id(input.runtime_event_id)?;
    validate_conversation_sequence(input.conversation_sequence)?;
    validate_required_text(&input.summary_label, "summaryLabel")?;
    validate_required_text(&input.created_at, "createdAt")?;
    validate_required_text(&input.completed_at, "completedAt")?;
    if input.files.is_empty() {
        return Err(CommandError::user_fixable(
            "code_patchset_invalid",
            "Field `files` must contain at least one patch file.",
        ));
    }
    let _ = epoch_to_sql(input.workspace_epoch)?;

    let mut file_ids = BTreeSet::new();
    for file in &input.files {
        validate_patch_file_input(file)?;
        if !file_ids.insert(file.patch_file_id.clone()) {
            return Err(CommandError::user_fixable(
                "code_patchset_invalid",
                format!(
                    "Patch file id `{}` appears more than once in patchset `{}`.",
                    file.patch_file_id, input.patchset_id
                ),
            ));
        }
    }

    Ok(())
}

fn validate_patch_file_input(file: &CodePatchFileInput) -> CommandResult<()> {
    validate_required_text(&file.patch_file_id, "files[].patchFileId")?;
    validate_optional_text(file.path_before.as_deref(), "files[].pathBefore")?;
    validate_optional_text(file.path_after.as_deref(), "files[].pathAfter")?;
    validate_optional_text(
        file.base_symlink_target.as_deref(),
        "files[].baseSymlinkTarget",
    )?;
    validate_optional_text(
        file.result_symlink_target.as_deref(),
        "files[].resultSymlinkTarget",
    )?;
    validate_optional_hash(file.base_hash.as_deref(), "files[].baseHash")?;
    validate_optional_hash(file.result_hash.as_deref(), "files[].resultHash")?;
    validate_optional_hash(file.base_blob_id.as_deref(), "files[].baseBlobId")?;
    validate_optional_hash(file.result_blob_id.as_deref(), "files[].resultBlobId")?;
    validate_file_operation_paths(file)?;

    if file.merge_policy != CodePatchMergePolicy::Text && !file.hunks.is_empty() {
        return Err(CommandError::user_fixable(
            "code_patchset_invalid",
            format!(
                "Patch file `{}` includes text hunks but uses exact merge policy.",
                file.patch_file_id
            ),
        ));
    }

    let mut hunk_ids = BTreeSet::new();
    for hunk in &file.hunks {
        validate_patch_hunk_input(hunk)?;
        if !hunk_ids.insert(hunk.hunk_id.clone()) {
            return Err(CommandError::user_fixable(
                "code_patchset_invalid",
                format!(
                    "Hunk id `{}` appears more than once in patch file `{}`.",
                    hunk.hunk_id, file.patch_file_id
                ),
            ));
        }
    }

    Ok(())
}

fn validate_file_operation_paths(file: &CodePatchFileInput) -> CommandResult<()> {
    match file.operation {
        CodePatchFileOperation::Create => {
            require_absent(file.path_before.as_deref(), "files[].pathBefore")?;
            require_present(file.path_after.as_deref(), "files[].pathAfter")
        }
        CodePatchFileOperation::Delete => {
            require_present(file.path_before.as_deref(), "files[].pathBefore")?;
            require_absent(file.path_after.as_deref(), "files[].pathAfter")
        }
        CodePatchFileOperation::Modify
        | CodePatchFileOperation::ModeChange
        | CodePatchFileOperation::SymlinkChange => {
            require_present(file.path_before.as_deref(), "files[].pathBefore")?;
            require_present(file.path_after.as_deref(), "files[].pathAfter")
        }
        CodePatchFileOperation::Rename => {
            require_present(file.path_before.as_deref(), "files[].pathBefore")?;
            require_present(file.path_after.as_deref(), "files[].pathAfter")?;
            if file.path_before == file.path_after {
                return Err(CommandError::user_fixable(
                    "code_patchset_invalid",
                    "Rename patch files must have different before and after paths.",
                ));
            }
            Ok(())
        }
    }
}

fn validate_patch_hunk_input(hunk: &CodePatchHunkInput) -> CommandResult<()> {
    validate_required_text(&hunk.hunk_id, "files[].hunks[].hunkId")?;
    let _ = u32_to_sql(hunk.hunk_index);
    let _ = u32_to_sql(hunk.base_start_line);
    let _ = u32_to_sql(hunk.base_line_count);
    let _ = u32_to_sql(hunk.result_start_line);
    let _ = u32_to_sql(hunk.result_line_count);
    Ok(())
}

fn validate_completed_change_group_for_commit(
    connection: &Connection,
    input: &CodePatchsetCommitInput,
    database_path: &Path,
) -> CommandResult<()> {
    let metadata = read_completed_change_group_metadata(
        connection,
        &input.project_id,
        &input.change_group_id,
        database_path,
    )?;
    if metadata.agent_session_id != input.agent_session_id || metadata.run_id != input.run_id {
        return Err(CommandError::user_fixable(
            "code_commit_change_group_mismatch",
            format!(
                "Code commit `{}` must use the same agent session and run as change group `{}`.",
                input.commit_id, input.change_group_id
            ),
        ));
    }
    Ok(())
}

fn read_completed_change_group_metadata(
    connection: &Connection,
    project_id: &str,
    change_group_id: &str,
    database_path: &Path,
) -> CommandResult<CompletedChangeGroupMetadata> {
    let (agent_session_id, run_id, status): (String, String, String) = connection
        .query_row(
            r#"
            SELECT agent_session_id, run_id, status
            FROM code_change_groups
            WHERE project_id = ?1
              AND change_group_id = ?2
            "#,
            params![project_id, change_group_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_change_group_read_failed", error)
        })?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "code_change_group_missing",
                format!("Xero could not find code change group `{change_group_id}`."),
            )
        })?;

    if status != "completed" {
        return Err(CommandError::user_fixable(
            "code_change_group_incomplete",
            format!(
                "Code change group `{change_group_id}` is `{status}` and cannot be committed to code history yet."
            ),
        ));
    }

    Ok(CompletedChangeGroupMetadata {
        agent_session_id,
        run_id,
    })
}

fn insert_code_patchset(
    connection: &Connection,
    input: &CodePatchsetCommitInput,
    database_path: &Path,
) -> CommandResult<()> {
    connection
        .execute(
            r#"
            INSERT INTO code_patchsets (
                project_id,
                patchset_id,
                change_group_id,
                base_commit_id,
                base_tree_id,
                result_tree_id,
                patch_kind,
                file_count,
                text_hunk_count,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            "#,
            params![
                input.project_id,
                input.patchset_id,
                input.change_group_id,
                input.parent_commit_id,
                input.parent_tree_id,
                input.tree_id,
                input.commit_kind.as_str(),
                usize_to_sql(input.files.len(), "fileCount")?,
                usize_to_sql(text_hunk_count(&input.files), "textHunkCount")?,
                input.created_at,
            ],
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_patchset_insert_failed", error)
        })?;
    Ok(())
}

fn insert_code_commit(
    connection: &Connection,
    input: &CodePatchsetCommitInput,
    database_path: &Path,
) -> CommandResult<()> {
    connection
        .execute(
            r#"
            INSERT INTO code_commits (
                project_id,
                commit_id,
                parent_commit_id,
                tree_id,
                parent_tree_id,
                patchset_id,
                change_group_id,
                history_operation_id,
                agent_session_id,
                run_id,
                tool_call_id,
                runtime_event_id,
                conversation_sequence,
                commit_kind,
                summary_label,
                workspace_epoch,
                created_at,
                completed_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)
            "#,
            params![
                input.project_id,
                input.commit_id,
                input.parent_commit_id,
                input.tree_id,
                input.parent_tree_id,
                input.patchset_id,
                input.change_group_id,
                input.history_operation_id,
                input.agent_session_id,
                input.run_id,
                input.tool_call_id,
                input.runtime_event_id,
                input.conversation_sequence,
                input.commit_kind.as_str(),
                input.summary_label,
                epoch_to_sql(input.workspace_epoch)?,
                input.created_at,
                input.completed_at,
            ],
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_commit_insert_failed", error)
        })?;
    Ok(())
}

fn insert_code_patch_file(
    connection: &Connection,
    input: &CodePatchsetCommitInput,
    file: &CodePatchFileInput,
    file_index: i64,
    database_path: &Path,
) -> CommandResult<()> {
    connection
        .execute(
            r#"
            INSERT INTO code_patch_files (
                project_id,
                patchset_id,
                patch_file_id,
                file_index,
                path_before,
                path_after,
                operation,
                merge_policy,
                before_file_kind,
                after_file_kind,
                base_hash,
                result_hash,
                base_blob_id,
                result_blob_id,
                base_size,
                result_size,
                base_mode,
                result_mode,
                base_symlink_target,
                result_symlink_target,
                text_hunk_count,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
            "#,
            params![
                input.project_id,
                input.patchset_id,
                file.patch_file_id,
                file_index,
                file.path_before,
                file.path_after,
                file.operation.as_str(),
                file.merge_policy.as_str(),
                file.before_file_kind.map(CodePatchFileKind::as_str),
                file.after_file_kind.map(CodePatchFileKind::as_str),
                file.base_hash,
                file.result_hash,
                file.base_blob_id,
                file.result_blob_id,
                optional_u64_to_sql(file.base_size, "baseSize")?,
                optional_u64_to_sql(file.result_size, "resultSize")?,
                file.base_mode.map(u32_to_sql),
                file.result_mode.map(u32_to_sql),
                file.base_symlink_target,
                file.result_symlink_target,
                usize_to_sql(file.hunks.len(), "textHunkCount")?,
                input.created_at,
            ],
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_patch_file_insert_failed", error)
        })?;
    Ok(())
}

fn insert_code_patch_hunk(
    connection: &Connection,
    input: &CodePatchsetCommitInput,
    file: &CodePatchFileInput,
    hunk: &CodePatchHunkInput,
    database_path: &Path,
) -> CommandResult<()> {
    connection
        .execute(
            r#"
            INSERT INTO code_patch_hunks (
                project_id,
                patch_file_id,
                hunk_id,
                hunk_index,
                base_start_line,
                base_line_count,
                result_start_line,
                result_line_count,
                removed_lines_json,
                added_lines_json,
                context_before_json,
                context_after_json,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
            "#,
            params![
                input.project_id,
                file.patch_file_id,
                hunk.hunk_id,
                u32_to_sql(hunk.hunk_index),
                u32_to_sql(hunk.base_start_line),
                u32_to_sql(hunk.base_line_count),
                u32_to_sql(hunk.result_start_line),
                u32_to_sql(hunk.result_line_count),
                encode_json_lines(&hunk.removed_lines, "removedLines")?,
                encode_json_lines(&hunk.added_lines, "addedLines")?,
                encode_json_lines(&hunk.context_before, "contextBefore")?,
                encode_json_lines(&hunk.context_after, "contextAfter")?,
                input.created_at,
            ],
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_patch_hunk_insert_failed", error)
        })?;
    Ok(())
}

fn read_code_commit_from_connection(
    connection: &Connection,
    project_id: &str,
    commit_id: &str,
    database_path: &Path,
) -> CommandResult<Option<CodeCommitRecord>> {
    let raw = connection
        .query_row(
            r#"
            SELECT
                project_id,
                commit_id,
                parent_commit_id,
                tree_id,
                parent_tree_id,
                patchset_id,
                change_group_id,
                history_operation_id,
                agent_session_id,
                run_id,
                tool_call_id,
                runtime_event_id,
                conversation_sequence,
                commit_kind,
                summary_label,
                workspace_epoch,
                created_at,
                completed_at
            FROM code_commits
            WHERE project_id = ?1
              AND commit_id = ?2
            "#,
            params![project_id, commit_id],
            raw_code_commit_from_row,
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_commit_read_failed", error)
        })?;
    raw.map(code_commit_from_raw).transpose()
}

fn read_code_patchset_from_connection(
    connection: &Connection,
    project_id: &str,
    patchset_id: &str,
    database_path: &Path,
) -> CommandResult<Option<CodePatchsetRecord>> {
    let raw = connection
        .query_row(
            r#"
            SELECT
                project_id,
                patchset_id,
                change_group_id,
                base_commit_id,
                base_tree_id,
                result_tree_id,
                patch_kind,
                file_count,
                text_hunk_count,
                created_at
            FROM code_patchsets
            WHERE project_id = ?1
              AND patchset_id = ?2
            "#,
            params![project_id, patchset_id],
            raw_code_patchset_from_row,
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_patchset_read_failed", error)
        })?;
    raw.map(code_patchset_from_raw).transpose()
}

fn read_code_patch_files_from_connection(
    connection: &Connection,
    project_id: &str,
    patchset_id: &str,
    database_path: &Path,
) -> CommandResult<Vec<CodePatchFileRecord>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                patchset_id,
                patch_file_id,
                file_index,
                path_before,
                path_after,
                operation,
                merge_policy,
                before_file_kind,
                after_file_kind,
                base_hash,
                result_hash,
                base_blob_id,
                result_blob_id,
                base_size,
                result_size,
                base_mode,
                result_mode,
                base_symlink_target,
                result_symlink_target,
                text_hunk_count,
                created_at
            FROM code_patch_files
            WHERE project_id = ?1
              AND patchset_id = ?2
            ORDER BY file_index ASC, patch_file_id ASC
            "#,
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_patch_file_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(
            params![project_id, patchset_id],
            raw_code_patch_file_from_row,
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_patch_file_read_failed", error)
        })?;

    let mut files = Vec::new();
    for row in rows {
        let raw = row.map_err(|error| {
            map_code_history_storage_error(database_path, "code_patch_file_read_failed", error)
        })?;
        let hunks = read_code_patch_hunks_from_connection(
            connection,
            project_id,
            &raw.patch_file_id,
            database_path,
        )?;
        files.push(code_patch_file_from_raw(raw, hunks)?);
    }
    Ok(files)
}

fn read_code_patch_hunks_from_connection(
    connection: &Connection,
    project_id: &str,
    patch_file_id: &str,
    database_path: &Path,
) -> CommandResult<Vec<CodePatchHunkRecord>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                project_id,
                patch_file_id,
                hunk_id,
                hunk_index,
                base_start_line,
                base_line_count,
                result_start_line,
                result_line_count,
                removed_lines_json,
                added_lines_json,
                context_before_json,
                context_after_json,
                created_at
            FROM code_patch_hunks
            WHERE project_id = ?1
              AND patch_file_id = ?2
            ORDER BY hunk_index ASC, hunk_id ASC
            "#,
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_patch_hunk_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(
            params![project_id, patch_file_id],
            raw_code_patch_hunk_from_row,
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_patch_hunk_read_failed", error)
        })?;

    let mut hunks = Vec::new();
    for row in rows {
        let raw = row.map_err(|error| {
            map_code_history_storage_error(database_path, "code_patch_hunk_read_failed", error)
        })?;
        hunks.push(code_patch_hunk_from_raw(raw)?);
    }
    Ok(hunks)
}

fn raw_code_commit_from_row(row: &Row<'_>) -> rusqlite::Result<RawCodeCommitRow> {
    Ok(RawCodeCommitRow {
        project_id: row.get(0)?,
        commit_id: row.get(1)?,
        parent_commit_id: row.get(2)?,
        tree_id: row.get(3)?,
        parent_tree_id: row.get(4)?,
        patchset_id: row.get(5)?,
        change_group_id: row.get(6)?,
        history_operation_id: row.get(7)?,
        agent_session_id: row.get(8)?,
        run_id: row.get(9)?,
        tool_call_id: row.get(10)?,
        runtime_event_id: row.get(11)?,
        conversation_sequence: row.get(12)?,
        commit_kind: row.get(13)?,
        summary_label: row.get(14)?,
        workspace_epoch: row.get(15)?,
        created_at: row.get(16)?,
        completed_at: row.get(17)?,
    })
}

fn raw_code_patchset_from_row(row: &Row<'_>) -> rusqlite::Result<RawCodePatchsetRow> {
    Ok(RawCodePatchsetRow {
        project_id: row.get(0)?,
        patchset_id: row.get(1)?,
        change_group_id: row.get(2)?,
        base_commit_id: row.get(3)?,
        base_tree_id: row.get(4)?,
        result_tree_id: row.get(5)?,
        patch_kind: row.get(6)?,
        file_count: row.get(7)?,
        text_hunk_count: row.get(8)?,
        created_at: row.get(9)?,
    })
}

fn raw_code_patch_file_from_row(row: &Row<'_>) -> rusqlite::Result<RawCodePatchFileRow> {
    Ok(RawCodePatchFileRow {
        project_id: row.get(0)?,
        patchset_id: row.get(1)?,
        patch_file_id: row.get(2)?,
        file_index: row.get(3)?,
        path_before: row.get(4)?,
        path_after: row.get(5)?,
        operation: row.get(6)?,
        merge_policy: row.get(7)?,
        before_file_kind: row.get(8)?,
        after_file_kind: row.get(9)?,
        base_hash: row.get(10)?,
        result_hash: row.get(11)?,
        base_blob_id: row.get(12)?,
        result_blob_id: row.get(13)?,
        base_size: row.get(14)?,
        result_size: row.get(15)?,
        base_mode: row.get(16)?,
        result_mode: row.get(17)?,
        base_symlink_target: row.get(18)?,
        result_symlink_target: row.get(19)?,
        text_hunk_count: row.get(20)?,
        created_at: row.get(21)?,
    })
}

fn raw_code_patch_hunk_from_row(row: &Row<'_>) -> rusqlite::Result<RawCodePatchHunkRow> {
    Ok(RawCodePatchHunkRow {
        project_id: row.get(0)?,
        patch_file_id: row.get(1)?,
        hunk_id: row.get(2)?,
        hunk_index: row.get(3)?,
        base_start_line: row.get(4)?,
        base_line_count: row.get(5)?,
        result_start_line: row.get(6)?,
        result_line_count: row.get(7)?,
        removed_lines_json: row.get(8)?,
        added_lines_json: row.get(9)?,
        context_before_json: row.get(10)?,
        context_after_json: row.get(11)?,
        created_at: row.get(12)?,
    })
}

fn code_commit_from_raw(raw: RawCodeCommitRow) -> CommandResult<CodeCommitRecord> {
    Ok(CodeCommitRecord {
        project_id: raw.project_id,
        commit_id: raw.commit_id,
        parent_commit_id: raw.parent_commit_id,
        tree_id: raw.tree_id,
        parent_tree_id: raw.parent_tree_id,
        patchset_id: raw.patchset_id,
        change_group_id: raw.change_group_id,
        history_operation_id: raw.history_operation_id,
        agent_session_id: raw.agent_session_id,
        run_id: raw.run_id,
        tool_call_id: raw.tool_call_id,
        runtime_event_id: raw.runtime_event_id,
        conversation_sequence: raw.conversation_sequence,
        commit_kind: CodeHistoryCommitKind::from_sql(&raw.commit_kind)?,
        summary_label: raw.summary_label,
        workspace_epoch: sql_to_u64(raw.workspace_epoch, "workspaceEpoch")?,
        created_at: raw.created_at,
        completed_at: raw.completed_at,
    })
}

fn code_patchset_from_raw(raw: RawCodePatchsetRow) -> CommandResult<CodePatchsetRecord> {
    Ok(CodePatchsetRecord {
        project_id: raw.project_id,
        patchset_id: raw.patchset_id,
        change_group_id: raw.change_group_id,
        base_commit_id: raw.base_commit_id,
        base_tree_id: raw.base_tree_id,
        result_tree_id: raw.result_tree_id,
        patch_kind: CodeHistoryCommitKind::from_sql(&raw.patch_kind)?,
        file_count: sql_to_u32(raw.file_count, "fileCount")?,
        text_hunk_count: sql_to_u32(raw.text_hunk_count, "textHunkCount")?,
        created_at: raw.created_at,
    })
}

fn code_patch_file_from_raw(
    raw: RawCodePatchFileRow,
    hunks: Vec<CodePatchHunkRecord>,
) -> CommandResult<CodePatchFileRecord> {
    Ok(CodePatchFileRecord {
        project_id: raw.project_id,
        patchset_id: raw.patchset_id,
        patch_file_id: raw.patch_file_id,
        file_index: sql_to_u32(raw.file_index, "fileIndex")?,
        path_before: raw.path_before,
        path_after: raw.path_after,
        operation: CodePatchFileOperation::from_sql(&raw.operation)?,
        merge_policy: CodePatchMergePolicy::from_sql(&raw.merge_policy)?,
        before_file_kind: raw
            .before_file_kind
            .as_deref()
            .map(CodePatchFileKind::from_sql)
            .transpose()?,
        after_file_kind: raw
            .after_file_kind
            .as_deref()
            .map(CodePatchFileKind::from_sql)
            .transpose()?,
        base_hash: raw.base_hash,
        result_hash: raw.result_hash,
        base_blob_id: raw.base_blob_id,
        result_blob_id: raw.result_blob_id,
        base_size: raw
            .base_size
            .map(|value| sql_to_u64(value, "baseSize"))
            .transpose()?,
        result_size: raw
            .result_size
            .map(|value| sql_to_u64(value, "resultSize"))
            .transpose()?,
        base_mode: raw
            .base_mode
            .map(|value| sql_to_u32(value, "baseMode"))
            .transpose()?,
        result_mode: raw
            .result_mode
            .map(|value| sql_to_u32(value, "resultMode"))
            .transpose()?,
        base_symlink_target: raw.base_symlink_target,
        result_symlink_target: raw.result_symlink_target,
        text_hunk_count: sql_to_u32(raw.text_hunk_count, "textHunkCount")?,
        created_at: raw.created_at,
        hunks,
    })
}

fn code_patch_hunk_from_raw(raw: RawCodePatchHunkRow) -> CommandResult<CodePatchHunkRecord> {
    Ok(CodePatchHunkRecord {
        project_id: raw.project_id,
        patch_file_id: raw.patch_file_id,
        hunk_id: raw.hunk_id.clone(),
        hunk_index: sql_to_u32(raw.hunk_index, "hunkIndex")?,
        base_start_line: sql_to_u32(raw.base_start_line, "baseStartLine")?,
        base_line_count: sql_to_u32(raw.base_line_count, "baseLineCount")?,
        result_start_line: sql_to_u32(raw.result_start_line, "resultStartLine")?,
        result_line_count: sql_to_u32(raw.result_line_count, "resultLineCount")?,
        removed_lines: decode_json_lines(&raw.removed_lines_json, "removedLines", &raw.hunk_id)?,
        added_lines: decode_json_lines(&raw.added_lines_json, "addedLines", &raw.hunk_id)?,
        context_before: decode_json_lines(&raw.context_before_json, "contextBefore", &raw.hunk_id)?,
        context_after: decode_json_lines(&raw.context_after_json, "contextAfter", &raw.hunk_id)?,
        created_at: raw.created_at,
    })
}

fn ensure_code_workspace_head_in_connection(
    connection: &Connection,
    project_id: &str,
    now: &str,
    database_path: &Path,
) -> CommandResult<()> {
    connection
        .execute(
            r#"
            INSERT INTO code_workspace_heads (
                project_id,
                head_id,
                tree_id,
                workspace_epoch,
                latest_history_operation_id,
                created_at,
                updated_at
            )
            SELECT ?1, NULL, NULL, 0, NULL, ?2, ?2
            WHERE EXISTS (SELECT 1 FROM projects WHERE id = ?1)
            ON CONFLICT(project_id) DO NOTHING
            "#,
            params![project_id, now],
        )
        .map_err(|error| {
            map_code_history_storage_error(
                database_path,
                "code_workspace_head_initialize_failed",
                error,
            )
        })?;

    if read_code_workspace_head_from_connection(connection, project_id, database_path)?.is_none() {
        return Err(missing_project_head_error(project_id));
    }

    Ok(())
}

fn read_code_workspace_head_from_connection(
    connection: &Connection,
    project_id: &str,
    database_path: &Path,
) -> CommandResult<Option<CodeWorkspaceHeadRecord>> {
    connection
        .query_row(
            r#"
            SELECT
                project_id,
                head_id,
                tree_id,
                workspace_epoch,
                latest_history_operation_id,
                created_at,
                updated_at
            FROM code_workspace_heads
            WHERE project_id = ?1
            "#,
            params![project_id],
            workspace_head_from_row,
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_workspace_head_read_failed", error)
        })
}

fn read_code_path_epochs_from_connection(
    connection: &Connection,
    project_id: &str,
    paths: &[String],
    database_path: &Path,
) -> CommandResult<Vec<CodePathEpochRecord>> {
    let mut path_epochs = Vec::with_capacity(paths.len());
    for path in paths {
        let Some(path_epoch) =
            read_code_path_epoch_from_connection(connection, project_id, path, database_path)?
        else {
            return Err(CommandError::system_fault(
                "code_path_epoch_missing",
                format!(
                    "Xero advanced code workspace epoch for `{path}` in project `{project_id}`, but the path epoch row could not be reloaded."
                ),
            ));
        };
        path_epochs.push(path_epoch);
    }
    Ok(path_epochs)
}

fn read_code_path_epoch_from_connection(
    connection: &Connection,
    project_id: &str,
    path: &str,
    database_path: &Path,
) -> CommandResult<Option<CodePathEpochRecord>> {
    connection
        .query_row(
            r#"
            SELECT
                project_id,
                path,
                workspace_epoch,
                commit_id,
                history_operation_id,
                created_at,
                updated_at
            FROM code_path_epochs
            WHERE project_id = ?1
              AND path = ?2
            "#,
            params![project_id, path],
            path_epoch_from_row,
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_path_epoch_read_failed", error)
        })
}

fn workspace_head_from_row(row: &Row<'_>) -> rusqlite::Result<CodeWorkspaceHeadRecord> {
    let workspace_epoch: i64 = row.get(3)?;
    Ok(CodeWorkspaceHeadRecord {
        project_id: row.get(0)?,
        head_id: row.get(1)?,
        tree_id: row.get(2)?,
        workspace_epoch: workspace_epoch as u64,
        latest_history_operation_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn path_epoch_from_row(row: &Row<'_>) -> rusqlite::Result<CodePathEpochRecord> {
    let workspace_epoch: i64 = row.get(2)?;
    Ok(CodePathEpochRecord {
        project_id: row.get(0)?,
        path: row.get(1)?,
        workspace_epoch: workspace_epoch as u64,
        commit_id: row.get(3)?,
        history_operation_id: row.get(4)?,
        created_at: row.get(5)?,
        updated_at: row.get(6)?,
    })
}

fn validate_advance_request(request: &AdvanceCodeWorkspaceEpochRequest) -> CommandResult<()> {
    validate_required_text(&request.project_id, "projectId")?;
    validate_optional_text(request.head_id.as_deref(), "headId")?;
    validate_optional_text(request.tree_id.as_deref(), "treeId")?;
    validate_optional_text(request.commit_id.as_deref(), "commitId")?;
    validate_optional_text(
        request.latest_history_operation_id.as_deref(),
        "latestHistoryOperationId",
    )?;
    validate_required_text(&request.updated_at, "updatedAt")?;
    let _ = normalize_affected_paths(&request.affected_paths)?;
    Ok(())
}

fn normalize_affected_paths(paths: &[String]) -> CommandResult<Vec<String>> {
    if paths.is_empty() {
        return Err(CommandError::user_fixable(
            "code_workspace_epoch_invalid",
            "Field `affectedPaths` must contain at least one project-relative path.",
        ));
    }

    let mut unique_paths = BTreeSet::new();
    for path in paths {
        validate_required_text(path, "affectedPaths[]")?;
        unique_paths.insert(path.clone());
    }

    Ok(unique_paths.into_iter().collect())
}

fn text_hunk_count(files: &[CodePatchFileInput]) -> usize {
    files.iter().map(|file| file.hunks.len()).sum()
}

fn require_present(value: Option<&str>, field: &str) -> CommandResult<()> {
    match value {
        Some(value) => validate_required_text(value, field),
        None => Err(CommandError::user_fixable(
            "code_patchset_invalid",
            format!("Field `{field}` is required for this patch file operation."),
        )),
    }
}

fn require_absent(value: Option<&str>, field: &str) -> CommandResult<()> {
    if value.is_some() {
        return Err(CommandError::user_fixable(
            "code_patchset_invalid",
            format!("Field `{field}` must be omitted for this patch file operation."),
        ));
    }
    Ok(())
}

fn validate_runtime_event_id(value: Option<i64>) -> CommandResult<()> {
    if value.is_some_and(|value| value <= 0) {
        return Err(CommandError::user_fixable(
            "code_history_storage_invalid",
            "Field `runtimeEventId` must be greater than zero when present.",
        ));
    }
    Ok(())
}

fn validate_conversation_sequence(value: Option<i64>) -> CommandResult<()> {
    if value.is_some_and(|value| value < 0) {
        return Err(CommandError::user_fixable(
            "code_history_storage_invalid",
            "Field `conversationSequence` must be zero or greater when present.",
        ));
    }
    Ok(())
}

fn validate_optional_text(value: Option<&str>, field: &str) -> CommandResult<()> {
    if let Some(value) = value {
        validate_required_text(value, field)?;
    }
    Ok(())
}

fn validate_optional_hash(value: Option<&str>, field: &str) -> CommandResult<()> {
    if let Some(value) = value {
        validate_required_text(value, field)?;
        if !is_lower_hex_sha256(value) {
            return Err(CommandError::user_fixable(
                "code_history_storage_invalid",
                format!("Field `{field}` must be a 64-character lowercase SHA-256 hex string."),
            ));
        }
    }
    Ok(())
}

fn validate_required_text(value: &str, field: &str) -> CommandResult<()> {
    if value.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "code_history_storage_invalid",
            format!("Field `{field}` must be a non-empty string."),
        ));
    }
    Ok(())
}

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn encode_json_lines(values: &[String], field: &str) -> CommandResult<String> {
    serde_json::to_string(values).map_err(|error| {
        CommandError::system_fault(
            "code_patch_hunk_encode_failed",
            format!("Xero could not encode `{field}` for a code patch hunk: {error}"),
        )
    })
}

fn decode_json_lines(raw: &str, field: &str, hunk_id: &str) -> CommandResult<Vec<String>> {
    serde_json::from_str(raw).map_err(|error| {
        decode_error(
            field,
            format!("Code patch hunk `{hunk_id}` has invalid `{field}` JSON: {error}"),
        )
    })
}

fn optional_u64_to_sql(value: Option<u64>, field: &str) -> CommandResult<Option<i64>> {
    value.map(|value| u64_to_sql(value, field)).transpose()
}

fn u64_to_sql(value: u64, field: &str) -> CommandResult<i64> {
    i64::try_from(value).map_err(|_| {
        CommandError::system_fault(
            "code_history_integer_overflow",
            format!("Xero could not persist `{field}` because it exceeds SQLite integer storage."),
        )
    })
}

fn usize_to_sql(value: usize, field: &str) -> CommandResult<i64> {
    i64::try_from(value).map_err(|_| {
        CommandError::system_fault(
            "code_history_integer_overflow",
            format!("Xero could not persist `{field}` because it exceeds SQLite integer storage."),
        )
    })
}

fn u32_to_sql(value: u32) -> i64 {
    i64::from(value)
}

fn sql_to_u64(value: i64, field: &str) -> CommandResult<u64> {
    u64::try_from(value).map_err(|_| {
        decode_error(
            field,
            format!("Field `{field}` contained a negative SQLite integer."),
        )
    })
}

fn sql_to_u32(value: i64, field: &str) -> CommandResult<u32> {
    u32::try_from(value).map_err(|_| {
        decode_error(
            field,
            format!("Field `{field}` could not fit in an unsigned 32-bit integer."),
        )
    })
}

fn epoch_to_sql(epoch: u64) -> CommandResult<i64> {
    i64::try_from(epoch).map_err(|_| {
        CommandError::system_fault(
            "code_workspace_epoch_overflow",
            "Xero could not persist the code workspace epoch because it exceeds SQLite integer storage.",
        )
    })
}

fn missing_project_head_error(project_id: &str) -> CommandError {
    CommandError::user_fixable(
        "code_workspace_project_missing",
        format!(
            "Xero could not find project `{project_id}` while initializing code history state."
        ),
    )
}

fn missing_commit_after_persist_error(commit_id: &str) -> CommandError {
    CommandError::system_fault(
        "code_commit_missing_after_persist",
        format!("Xero persisted code commit `{commit_id}`, but the commit could not be read back."),
    )
}

fn missing_patchset_for_commit_error(commit: &CodeCommitRecord) -> CommandError {
    CommandError::system_fault(
        "code_patchset_missing_for_commit",
        format!(
            "Code commit `{}` references patchset `{}`, but the patchset row is missing.",
            commit.commit_id, commit.patchset_id
        ),
    )
}

fn decode_error(field: &str, message: String) -> CommandError {
    CommandError::system_fault(
        "code_history_storage_decode_failed",
        format!("Xero could not decode code history storage field `{field}`: {message}"),
    )
}

fn map_code_history_storage_error(
    database_path: &Path,
    code: &'static str,
    error: rusqlite::Error,
) -> CommandError {
    CommandError::system_fault(
        code,
        format!(
            "Xero could not update code history storage at {}: {error}",
            database_path.display()
        ),
    )
}
