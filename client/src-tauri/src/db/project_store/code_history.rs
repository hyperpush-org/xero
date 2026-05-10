use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
};

use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sha2::{Digest, Sha256};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandErrorClass, CommandResult},
    db::database_path_for_repo,
};

use super::{open_project_database, read_project_row};

const DEFAULT_SESSION_ROLLBACK_COMMIT_SCAN_LIMIT: usize = 2_000;
const DEFAULT_SESSION_ROLLBACK_TARGET_CHANGE_GROUP_LIMIT: usize = 512;
const DEFAULT_SESSION_ROLLBACK_EXCLUDED_CHANGE_GROUP_LIMIT: usize = 512;
const DEFAULT_SESSION_ROLLBACK_PATCH_FILE_LIMIT: usize = 2_000;
const DEFAULT_SESSION_ROLLBACK_AFFECTED_PATH_LIMIT: usize = 4_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CodeSessionLineageUndoPlanBudget {
    max_commits_after_boundary: usize,
    max_target_change_groups: usize,
    max_excluded_change_groups: usize,
    max_patch_files: usize,
    max_affected_paths: usize,
}

impl Default for CodeSessionLineageUndoPlanBudget {
    fn default() -> Self {
        Self {
            max_commits_after_boundary: DEFAULT_SESSION_ROLLBACK_COMMIT_SCAN_LIMIT,
            max_target_change_groups: DEFAULT_SESSION_ROLLBACK_TARGET_CHANGE_GROUP_LIMIT,
            max_excluded_change_groups: DEFAULT_SESSION_ROLLBACK_EXCLUDED_CHANGE_GROUP_LIMIT,
            max_patch_files: DEFAULT_SESSION_ROLLBACK_PATCH_FILE_LIMIT,
            max_affected_paths: DEFAULT_SESSION_ROLLBACK_AFFECTED_PATH_LIMIT,
        }
    }
}

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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodePatchAvailabilityRecord {
    pub project_id: String,
    pub target_change_group_id: String,
    pub available: bool,
    pub affected_paths: Vec<String>,
    pub file_change_count: u32,
    pub text_hunk_count: u32,
    #[serde(default)]
    pub text_hunks: Vec<CodePatchTextHunkAvailabilityRecord>,
    pub unavailable_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodePatchTextHunkAvailabilityRecord {
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
pub struct CodeChangeGroupHistoryMetadataRecord {
    pub project_id: String,
    pub target_change_group_id: String,
    pub commit_id: Option<String>,
    pub workspace_epoch: Option<u64>,
    pub patch_availability: CodePatchAvailabilityRecord,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeSessionBoundaryTargetKind {
    SessionBoundary,
    RunBoundary,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeSessionBoundarySourceKind {
    ChangeGroup,
    Checkpoint,
    Event,
    FileChange,
    Message,
    Run,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolveCodeSessionBoundaryRequest {
    pub project_id: String,
    pub agent_session_id: String,
    pub target_kind: CodeSessionBoundaryTargetKind,
    pub target_id: String,
    pub boundary_id: String,
    pub run_id: Option<String>,
    pub change_group_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSessionBoundaryLineageRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub target_kind: CodeSessionBoundaryTargetKind,
    pub target_run_id: Option<String>,
    pub boundary_run_id: String,
    pub root_run_id: String,
    pub parent_run_id: Option<String>,
    pub included_run_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCodeSessionBoundary {
    pub project_id: String,
    pub agent_session_id: String,
    pub target_kind: CodeSessionBoundaryTargetKind,
    pub target_id: String,
    pub boundary_id: String,
    pub source_kind: CodeSessionBoundarySourceKind,
    pub source_id: String,
    pub boundary_run_id: String,
    pub boundary_created_at: Option<String>,
    pub boundary_change_group_id: Option<String>,
    pub commit: CodeCommitRecord,
    pub lineage: CodeSessionBoundaryLineageRecord,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildCodeSessionLineageUndoPlanRequest {
    pub boundary: ResolveCodeSessionBoundaryRequest,
    pub explicitly_selected_change_group_ids: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSessionLineageUndoPlanExclusionReason {
    SiblingSession,
    OutsideRunLineage,
    UserOrRecoveredChange,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSessionLineageUndoPlanChangeGroup {
    pub change_group_id: String,
    pub commit_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub commit_kind: CodeHistoryCommitKind,
    pub workspace_epoch: u64,
    pub completed_at: String,
    pub summary_label: String,
    pub explicitly_selected: bool,
    pub affected_paths: Vec<String>,
    pub commit: CodePatchsetCommitRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSessionLineageUndoPlanStatus {
    Clean,
    Conflicted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSessionLineageUndoPlanConflictKind {
    TextOverlap,
    FileMissing,
    PathAlreadyExists,
    PathMissing,
    CurrentStateMismatch,
    UnsupportedOperation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSessionLineageUndoPlanConflict {
    pub change_group_id: String,
    pub commit_id: String,
    pub patch_file_id: String,
    pub path: String,
    pub kind: CodeSessionLineageUndoPlanConflictKind,
    pub message: String,
    pub base_hash: Option<String>,
    pub selected_hash: Option<String>,
    pub current_hash: Option<String>,
    pub hunk_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSessionLineageUndoPlanDirtyOverlay {
    pub change_group_id: String,
    pub commit_id: String,
    pub patch_file_id: String,
    pub path: String,
    pub current_hash: Option<String>,
    pub selected_hash: Option<String>,
    pub planned_hash: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSessionLineageUndoPlanFile {
    pub change_group_id: String,
    pub commit_id: String,
    pub patch_file_id: String,
    pub path_before: Option<String>,
    pub path_after: Option<String>,
    pub operation: CodePatchFileOperation,
    pub merge_policy: CodePatchMergePolicy,
    pub current_hash: Option<String>,
    pub planned_hash: Option<String>,
    pub planned_content: Option<String>,
    pub file_actions: Vec<CodeFileOperationInverseAction>,
    pub inverse_hunk_ids: Vec<String>,
    pub preserved_dirty_overlay: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSessionLineageUndoPlanExcludedChangeGroup {
    pub change_group_id: String,
    pub commit_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub commit_kind: CodeHistoryCommitKind,
    pub workspace_epoch: u64,
    pub completed_at: String,
    pub summary_label: String,
    pub reason: CodeSessionLineageUndoPlanExclusionReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSessionLineageUndoPlan {
    pub project_id: String,
    pub agent_session_id: String,
    pub boundary: ResolvedCodeSessionBoundary,
    pub status: CodeSessionLineageUndoPlanStatus,
    pub target_change_groups: Vec<CodeSessionLineageUndoPlanChangeGroup>,
    pub excluded_change_groups: Vec<CodeSessionLineageUndoPlanExcludedChangeGroup>,
    pub affected_paths: Vec<String>,
    pub planned_files: Vec<CodeSessionLineageUndoPlanFile>,
    pub preserved_dirty_overlays: Vec<CodeSessionLineageUndoPlanDirtyOverlay>,
    pub conflicts: Vec<CodeSessionLineageUndoPlanConflict>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeTextInversePatchPlanStatus {
    Clean,
    Conflicted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeTextInversePatchConflictKind {
    TextOverlap,
    FileMissing,
    UnsupportedOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeTextInversePatchConflict {
    pub path: String,
    pub kind: CodeTextInversePatchConflictKind,
    pub message: String,
    pub base_hash: Option<String>,
    pub selected_hash: Option<String>,
    pub current_hash: Option<String>,
    pub hunk_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeTextInverseHunkPlan {
    pub source_hunk_id: String,
    pub current_start_line: u32,
    pub current_line_count: u32,
    pub inverse_removed_lines: Vec<String>,
    pub inverse_added_lines: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeTextInversePatchPlan {
    pub path: String,
    pub status: CodeTextInversePatchPlanStatus,
    pub current_hash: Option<String>,
    pub planned_hash: Option<String>,
    pub planned_content: Option<String>,
    pub inverse_hunks: Vec<CodeTextInverseHunkPlan>,
    pub conflicts: Vec<CodeTextInversePatchConflict>,
}

impl CodeTextInversePatchPlan {
    pub fn is_clean(&self) -> bool {
        self.status == CodeTextInversePatchPlanStatus::Clean
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeExactFileState {
    pub kind: CodePatchFileKind,
    pub content_hash: Option<String>,
    pub blob_id: Option<String>,
    pub size: Option<u64>,
    pub mode: Option<u32>,
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeFileOperationCurrentState {
    pub path_before: Option<CodeExactFileState>,
    pub path_after: Option<CodeExactFileState>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeFileOperationInversePatchPlanStatus {
    Clean,
    Conflicted,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeFileOperationInverseConflictKind {
    CurrentStateMismatch,
    PathAlreadyExists,
    PathMissing,
    UnsupportedOperation,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeFileOperationInverseConflict {
    pub path: String,
    pub kind: CodeFileOperationInverseConflictKind,
    pub message: String,
    pub expected_state: Option<CodeExactFileState>,
    pub current_state: Option<CodeExactFileState>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CodeFileOperationInverseActionKind {
    RemovePath,
    RenamePath,
    RestoreMode,
    RestorePath,
    RestoreSymlink,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeFileOperationInverseAction {
    pub kind: CodeFileOperationInverseActionKind,
    pub source_path: Option<String>,
    pub target_path: String,
    pub restore_state: Option<CodeExactFileState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeFileOperationInversePatchPlan {
    pub path_before: Option<String>,
    pub path_after: Option<String>,
    pub operation: CodePatchFileOperation,
    pub status: CodeFileOperationInversePatchPlanStatus,
    pub actions: Vec<CodeFileOperationInverseAction>,
    pub conflicts: Vec<CodeFileOperationInverseConflict>,
}

impl CodeFileOperationInversePatchPlan {
    pub fn is_clean(&self) -> bool {
        self.status == CodeFileOperationInversePatchPlanStatus::Clean
    }
}

pub fn plan_text_file_inverse_patch(
    file: &CodePatchFileRecord,
    current_content: Option<&str>,
) -> CodeTextInversePatchPlan {
    let path = patch_file_display_path(file);
    let current_hash = current_content.map(|content| sha256_hex(content.as_bytes()));
    let conflict_base = TextInverseConflictBase {
        path: path.clone(),
        base_hash: file.base_hash.clone(),
        selected_hash: file.result_hash.clone(),
        current_hash: current_hash.clone(),
    };

    if file.operation != CodePatchFileOperation::Modify
        || file.merge_policy != CodePatchMergePolicy::Text
        || file.hunks.is_empty()
    {
        return conflicted_text_inverse_plan(
            path,
            current_hash,
            vec![conflict_base.conflict(
                CodeTextInversePatchConflictKind::UnsupportedOperation,
                "Only text modify patch files can be planned by the text inverse patch planner.",
                file.hunks.iter().map(|hunk| hunk.hunk_id.clone()).collect(),
            )],
        );
    }

    let Some(current_content) = current_content else {
        return conflicted_text_inverse_plan(
            path,
            current_hash,
            vec![conflict_base.conflict(
                CodeTextInversePatchConflictKind::FileMissing,
                "The current file is missing, so the selected text change cannot be undone safely.",
                file.hunks.iter().map(|hunk| hunk.hunk_id.clone()).collect(),
            )],
        );
    };

    if file
        .base_hash
        .as_deref()
        .is_some_and(|base_hash| Some(base_hash) == current_hash.as_deref())
    {
        return clean_text_inverse_plan(
            path,
            current_hash.clone(),
            current_content.into(),
            Vec::new(),
        );
    }

    let mut working_lines = split_text_lines_preserving_endings(current_content);
    let mut inverse_hunks = Vec::new();
    let mut conflicts = Vec::new();
    let mut hunks = file.hunks.iter().collect::<Vec<_>>();
    hunks.sort_by(|left, right| {
        right
            .result_start_line
            .cmp(&left.result_start_line)
            .then_with(|| right.hunk_index.cmp(&left.hunk_index))
            .then_with(|| right.hunk_id.cmp(&left.hunk_id))
    });

    for hunk in hunks {
        let Some(start_index) = locate_inverse_hunk(&working_lines, hunk) else {
            conflicts.push(conflict_base.conflict(
                CodeTextInversePatchConflictKind::TextOverlap,
                "The current content changed the selected text or made the inverse hunk ambiguous.",
                vec![hunk.hunk_id.clone()],
            ));
            continue;
        };
        let removed_line_count = hunk.added_lines.len();
        if inverse_hunk_would_join_unrelated_lines(
            start_index,
            removed_line_count,
            &hunk.removed_lines,
            &working_lines,
        ) {
            conflicts.push(conflict_base.conflict(
                CodeTextInversePatchConflictKind::TextOverlap,
                "Undoing this hunk would join later current content onto a line without a trailing newline.",
                vec![hunk.hunk_id.clone()],
            ));
            continue;
        }

        working_lines.splice(
            start_index..start_index + removed_line_count,
            hunk.removed_lines.clone(),
        );
        inverse_hunks.push(CodeTextInverseHunkPlan {
            source_hunk_id: hunk.hunk_id.clone(),
            current_start_line: index_to_start_line(start_index),
            current_line_count: usize_to_u32_saturating(removed_line_count),
            inverse_removed_lines: hunk.added_lines.clone(),
            inverse_added_lines: hunk.removed_lines.clone(),
        });
    }

    if !conflicts.is_empty() {
        return conflicted_text_inverse_plan(path, current_hash, conflicts);
    }

    inverse_hunks.sort_by(|left, right| {
        left.current_start_line
            .cmp(&right.current_start_line)
            .then_with(|| left.source_hunk_id.cmp(&right.source_hunk_id))
    });
    let planned_content = working_lines.concat();
    clean_text_inverse_plan(path, current_hash, planned_content, inverse_hunks)
}

pub fn plan_file_operation_inverse_patch(
    file: &CodePatchFileRecord,
    current: &CodeFileOperationCurrentState,
) -> CodeFileOperationInversePatchPlan {
    match file.operation {
        CodePatchFileOperation::Create => plan_create_inverse(file, current),
        CodePatchFileOperation::Delete => plan_delete_inverse(file, current),
        CodePatchFileOperation::Rename => plan_rename_inverse(file, current),
        CodePatchFileOperation::Modify => plan_modify_inverse(file, current),
        CodePatchFileOperation::ModeChange => plan_metadata_inverse(
            file,
            current,
            CodeFileOperationInverseActionKind::RestoreMode,
            "The current mode or file state differs from the selected mode-change result.",
        ),
        CodePatchFileOperation::SymlinkChange => plan_metadata_inverse(
            file,
            current,
            CodeFileOperationInverseActionKind::RestoreSymlink,
            "The current symlink state differs from the selected symlink-change result.",
        ),
    }
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

pub fn validate_code_workspace_epoch_for_paths(
    repo_root: &Path,
    project_id: &str,
    observed_workspace_epoch: u64,
    paths: &[String],
) -> CommandResult<()> {
    validate_required_text(project_id, "projectId")?;
    let normalized_paths = normalize_code_workspace_epoch_paths(paths)?;
    if normalized_paths.is_empty() {
        return Ok(());
    }

    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let mut stale_paths = Vec::new();
    for path in normalized_paths {
        let Some(path_epoch) =
            read_code_path_epoch_from_connection(&connection, project_id, &path, &database_path)?
        else {
            continue;
        };
        if observed_workspace_epoch < path_epoch.workspace_epoch {
            stale_paths.push((path, path_epoch));
        }
    }

    if stale_paths.is_empty() {
        return Ok(());
    }

    let conflict_summary = stale_paths
        .iter()
        .take(5)
        .map(|(path, path_epoch)| {
            let history_operation = path_epoch
                .history_operation_id
                .as_deref()
                .map(|operation_id| format!(" by history operation `{operation_id}`"))
                .unwrap_or_default();
            format!(
                "`{path}` advanced to workspace epoch {}{history_operation} after this run last observed epoch {observed_workspace_epoch}",
                path_epoch.workspace_epoch
            )
        })
        .collect::<Vec<_>>()
        .join("; ");

    Err(CommandError::new(
        "agent_workspace_epoch_stale",
        CommandErrorClass::PolicyDenied,
        format!(
            "Xero refused this mutating tool call because code history changed overlapping path(s) since the run's last context refresh: {conflict_summary}. Re-read current files after the refreshed context and retry the write."
        ),
        false,
    ))
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
    read_code_patchset_commit_for_commit_from_connection(
        &connection,
        project_id,
        commit,
        &database_path,
    )
    .map(Some)
}

pub fn read_code_change_group_history_metadata(
    repo_root: &Path,
    project_id: &str,
    change_group_id: &str,
) -> CommandResult<Option<CodeChangeGroupHistoryMetadataRecord>> {
    validate_required_text(project_id, "projectId")?;
    validate_required_text(change_group_id, "changeGroupId")?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    let commit_id = connection
        .query_row(
            r#"
            SELECT commit_id
            FROM code_commits
            WHERE project_id = ?1
              AND change_group_id = ?2
            ORDER BY workspace_epoch DESC, completed_at DESC, commit_id DESC
            LIMIT 1
            "#,
            params![project_id, change_group_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(&database_path, "code_commit_lookup_failed", error)
        })?;

    let Some(commit_id) = commit_id else {
        return Ok(Some(unavailable_code_change_group_history_metadata(
            project_id,
            change_group_id,
            "No patchset commit has been recorded for this change group.",
        )));
    };
    read_code_patchset_commit(repo_root, project_id, &commit_id)
        .map(|record| record.map(|record| code_change_group_history_metadata_from_commit(&record)))
}

pub fn code_change_group_history_metadata_from_commit(
    record: &CodePatchsetCommitRecord,
) -> CodeChangeGroupHistoryMetadataRecord {
    CodeChangeGroupHistoryMetadataRecord {
        project_id: record.commit.project_id.clone(),
        target_change_group_id: record.commit.change_group_id.clone(),
        commit_id: Some(record.commit.commit_id.clone()),
        workspace_epoch: Some(record.commit.workspace_epoch),
        patch_availability: code_patch_availability_from_commit(record),
    }
}

pub fn resolve_code_session_boundary(
    repo_root: &Path,
    request: &ResolveCodeSessionBoundaryRequest,
) -> CommandResult<ResolvedCodeSessionBoundary> {
    validate_resolve_code_session_boundary_request(request)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &request.project_id)?;

    if let Some(change_group_id) = request.change_group_id.as_deref() {
        return resolve_code_session_boundary_change_group(
            &connection,
            &database_path,
            request,
            change_group_id,
        );
    }

    match parse_code_session_boundary_reference(request)? {
        ParsedCodeSessionBoundaryReference::ChangeGroup(change_group_id) => {
            resolve_code_session_boundary_change_group(
                &connection,
                &database_path,
                request,
                &change_group_id,
            )
        }
        ParsedCodeSessionBoundaryReference::Checkpoint(checkpoint_id) => {
            let source = read_checkpoint_boundary_source(
                &connection,
                &database_path,
                &request.project_id,
                checkpoint_id,
            )?;
            resolve_code_session_boundary_source(&connection, &database_path, request, source)
        }
        ParsedCodeSessionBoundaryReference::Event(event_id) => {
            let source = read_event_boundary_source(
                &connection,
                &database_path,
                &request.project_id,
                event_id,
            )?;
            resolve_code_session_boundary_source(&connection, &database_path, request, source)
        }
        ParsedCodeSessionBoundaryReference::FileChange(file_change_id) => {
            let source = read_file_change_boundary_source(
                &connection,
                &database_path,
                &request.project_id,
                file_change_id,
            )?;
            resolve_code_session_boundary_source(&connection, &database_path, request, source)
        }
        ParsedCodeSessionBoundaryReference::Message(message_id) => {
            let source = read_message_boundary_source(
                &connection,
                &database_path,
                &request.project_id,
                message_id,
            )?;
            resolve_code_session_boundary_source(&connection, &database_path, request, source)
        }
        ParsedCodeSessionBoundaryReference::Run(run_id) => {
            resolve_code_session_boundary_run(&connection, &database_path, request, &run_id)
        }
    }
}

pub fn build_code_session_lineage_undo_plan(
    repo_root: &Path,
    request: &BuildCodeSessionLineageUndoPlanRequest,
) -> CommandResult<CodeSessionLineageUndoPlan> {
    build_code_session_lineage_undo_plan_with_budget(
        repo_root,
        request,
        CodeSessionLineageUndoPlanBudget::default(),
    )
}

fn build_code_session_lineage_undo_plan_with_budget(
    repo_root: &Path,
    request: &BuildCodeSessionLineageUndoPlanRequest,
    plan_budget: CodeSessionLineageUndoPlanBudget,
) -> CommandResult<CodeSessionLineageUndoPlan> {
    validate_build_code_session_lineage_undo_plan_request(request)?;
    let explicit_change_group_ids =
        normalize_explicit_plan_change_group_ids(&request.explicitly_selected_change_group_ids)?;
    let boundary = resolve_code_session_boundary(repo_root, &request.boundary)?;
    let database_path = database_path_for_repo(repo_root);
    let connection = open_project_database(repo_root, &database_path)?;
    read_project_row(&connection, &database_path, repo_root, &boundary.project_id)?;

    let commits = read_code_commits_after_boundary_from_connection(
        &connection,
        &boundary.project_id,
        boundary.commit.workspace_epoch,
        plan_budget.max_commits_after_boundary,
        &database_path,
    )?;
    let explicit_set = explicit_change_group_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut matched_explicit_change_group_ids = BTreeSet::new();
    let mut seen_change_group_ids = BTreeSet::new();
    let mut target_change_groups = Vec::new();
    let mut excluded_change_groups = Vec::new();
    let mut affected_paths = BTreeSet::new();
    let mut target_patch_file_count = 0usize;

    for commit in commits {
        if !seen_change_group_ids.insert(commit.change_group_id.clone()) {
            continue;
        }

        let explicitly_selected = explicit_set.contains(&commit.change_group_id);
        if explicitly_selected {
            matched_explicit_change_group_ids.insert(commit.change_group_id.clone());
        }

        if explicitly_selected
            || code_commit_is_default_lineage_undo_target(&commit, &boundary.lineage)
        {
            if target_change_groups.len() >= plan_budget.max_target_change_groups {
                return Err(code_session_lineage_undo_plan_budget_exceeded(
                    "targetChangeGroups",
                    plan_budget.max_target_change_groups,
                ));
            }
            let patchset_commit = read_code_patchset_commit_for_commit_from_connection(
                &connection,
                &boundary.project_id,
                commit.clone(),
                &database_path,
            )?;
            let Some(next_patch_file_count) =
                target_patch_file_count.checked_add(patchset_commit.files.len())
            else {
                return Err(code_session_lineage_undo_plan_budget_exceeded(
                    "patchFiles",
                    plan_budget.max_patch_files,
                ));
            };
            if next_patch_file_count > plan_budget.max_patch_files {
                return Err(code_session_lineage_undo_plan_budget_exceeded(
                    "patchFiles",
                    plan_budget.max_patch_files,
                ));
            }
            target_patch_file_count = next_patch_file_count;
            let commit_affected_paths = affected_paths_for_patch_records(&patchset_commit.files);
            affected_paths.extend(commit_affected_paths.iter().cloned());
            if affected_paths.len() > plan_budget.max_affected_paths {
                return Err(code_session_lineage_undo_plan_budget_exceeded(
                    "affectedPaths",
                    plan_budget.max_affected_paths,
                ));
            }
            target_change_groups.push(CodeSessionLineageUndoPlanChangeGroup {
                change_group_id: commit.change_group_id.clone(),
                commit_id: commit.commit_id.clone(),
                agent_session_id: commit.agent_session_id.clone(),
                run_id: commit.run_id.clone(),
                commit_kind: commit.commit_kind,
                workspace_epoch: commit.workspace_epoch,
                completed_at: commit.completed_at.clone(),
                summary_label: commit.summary_label.clone(),
                explicitly_selected,
                affected_paths: commit_affected_paths,
                commit: patchset_commit,
            });
        } else if let Some(reason) =
            code_session_lineage_undo_plan_exclusion_reason(&commit, &boundary.lineage)
        {
            if excluded_change_groups.len() < plan_budget.max_excluded_change_groups {
                excluded_change_groups.push(CodeSessionLineageUndoPlanExcludedChangeGroup {
                    change_group_id: commit.change_group_id,
                    commit_id: commit.commit_id,
                    agent_session_id: commit.agent_session_id,
                    run_id: commit.run_id,
                    commit_kind: commit.commit_kind,
                    workspace_epoch: commit.workspace_epoch,
                    completed_at: commit.completed_at,
                    summary_label: commit.summary_label,
                    reason,
                });
            }
        }
    }

    let missing_explicit_change_group_ids = explicit_change_group_ids
        .iter()
        .filter(|change_group_id| !matched_explicit_change_group_ids.contains(*change_group_id))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_explicit_change_group_ids.is_empty() {
        return Err(CommandError::user_fixable(
            "code_session_lineage_undo_plan_explicit_missing",
            format!(
                "The explicitly selected change group id(s) are not later than the rollback boundary or do not exist: {}.",
                missing_explicit_change_group_ids.join(", ")
            ),
        ));
    }

    let current_workspace_plan =
        plan_session_lineage_current_workspace_overlay(repo_root, &target_change_groups)?;
    let status = if current_workspace_plan.conflicts.is_empty() {
        CodeSessionLineageUndoPlanStatus::Clean
    } else {
        CodeSessionLineageUndoPlanStatus::Conflicted
    };

    Ok(CodeSessionLineageUndoPlan {
        project_id: boundary.project_id.clone(),
        agent_session_id: boundary.agent_session_id.clone(),
        boundary,
        status,
        target_change_groups,
        excluded_change_groups,
        affected_paths: affected_paths.into_iter().collect(),
        planned_files: current_workspace_plan.planned_files,
        preserved_dirty_overlays: current_workspace_plan.preserved_dirty_overlays,
        conflicts: current_workspace_plan.conflicts,
    })
}

#[derive(Debug)]
struct SessionLineageCurrentWorkspacePlan {
    planned_files: Vec<CodeSessionLineageUndoPlanFile>,
    preserved_dirty_overlays: Vec<CodeSessionLineageUndoPlanDirtyOverlay>,
    conflicts: Vec<CodeSessionLineageUndoPlanConflict>,
}

fn plan_session_lineage_current_workspace_overlay(
    repo_root: &Path,
    target_change_groups: &[CodeSessionLineageUndoPlanChangeGroup],
) -> CommandResult<SessionLineageCurrentWorkspacePlan> {
    let mut text_overlays = BTreeMap::<String, Option<String>>::new();
    let mut exact_overlays = BTreeMap::<String, Option<CodeExactFileState>>::new();
    let mut planned_files = Vec::new();
    let mut preserved_dirty_overlays = Vec::new();
    let mut conflicts = Vec::new();

    for group in target_change_groups {
        for file in &group.commit.files {
            if file.operation == CodePatchFileOperation::Modify
                && file.merge_policy == CodePatchMergePolicy::Text
            {
                plan_session_lineage_text_inverse_overlay(
                    repo_root,
                    group,
                    file,
                    &mut text_overlays,
                    &mut exact_overlays,
                    &mut planned_files,
                    &mut preserved_dirty_overlays,
                    &mut conflicts,
                )?;
            } else {
                plan_session_lineage_exact_inverse_overlay(
                    repo_root,
                    group,
                    file,
                    &mut text_overlays,
                    &mut exact_overlays,
                    &mut planned_files,
                    &mut conflicts,
                )?;
            }
        }
    }

    Ok(SessionLineageCurrentWorkspacePlan {
        planned_files,
        preserved_dirty_overlays,
        conflicts,
    })
}

#[expect(
    clippy::too_many_arguments,
    reason = "overlay planning mutates several independent accumulators"
)]
fn plan_session_lineage_text_inverse_overlay(
    repo_root: &Path,
    group: &CodeSessionLineageUndoPlanChangeGroup,
    file: &CodePatchFileRecord,
    text_overlays: &mut BTreeMap<String, Option<String>>,
    exact_overlays: &mut BTreeMap<String, Option<CodeExactFileState>>,
    planned_files: &mut Vec<CodeSessionLineageUndoPlanFile>,
    preserved_dirty_overlays: &mut Vec<CodeSessionLineageUndoPlanDirtyOverlay>,
    conflicts: &mut Vec<CodeSessionLineageUndoPlanConflict>,
) -> CommandResult<()> {
    let path = patch_file_display_path(file);
    let current_content = current_text_for_session_lineage_plan(repo_root, text_overlays, &path)?;
    let plan = plan_text_file_inverse_patch(file, current_content.as_deref());

    if plan.status == CodeTextInversePatchPlanStatus::Conflicted {
        conflicts.extend(
            plan.conflicts
                .iter()
                .map(|conflict| session_lineage_conflict_from_text(group, file, conflict)),
        );
        return Ok(());
    }

    let planned_content = plan
        .planned_content
        .clone()
        .unwrap_or_else(|| current_content.clone().unwrap_or_else(|| String::from("")));
    let current_hash = plan.current_hash.clone();
    let planned_hash = plan.planned_hash.clone();
    let preserved_dirty_overlay = current_hash_is_preserved_overlay(current_hash.as_deref(), file);

    if preserved_dirty_overlay {
        preserved_dirty_overlays.push(CodeSessionLineageUndoPlanDirtyOverlay {
            change_group_id: group.change_group_id.clone(),
            commit_id: group.commit_id.clone(),
            patch_file_id: file.patch_file_id.clone(),
            path: path.clone(),
            current_hash: current_hash.clone(),
            selected_hash: file.result_hash.clone(),
            planned_hash: planned_hash.clone(),
        });
    }

    let inverse_hunk_ids = plan
        .inverse_hunks
        .iter()
        .map(|hunk| hunk.source_hunk_id.clone())
        .collect::<Vec<_>>();
    planned_files.push(CodeSessionLineageUndoPlanFile {
        change_group_id: group.change_group_id.clone(),
        commit_id: group.commit_id.clone(),
        patch_file_id: file.patch_file_id.clone(),
        path_before: file.path_before.clone(),
        path_after: file.path_after.clone(),
        operation: file.operation,
        merge_policy: file.merge_policy,
        current_hash: current_hash.clone(),
        planned_hash: planned_hash.clone(),
        planned_content: Some(planned_content.clone()),
        file_actions: Vec::new(),
        inverse_hunk_ids,
        preserved_dirty_overlay,
    });

    text_overlays.insert(path.clone(), Some(planned_content.clone()));
    let current_state =
        current_exact_state_for_session_lineage_plan(repo_root, exact_overlays, &path)?;
    exact_overlays.insert(
        path,
        Some(exact_state_for_planned_text(
            &planned_content,
            current_state.as_ref(),
        )),
    );
    Ok(())
}

fn plan_session_lineage_exact_inverse_overlay(
    repo_root: &Path,
    group: &CodeSessionLineageUndoPlanChangeGroup,
    file: &CodePatchFileRecord,
    text_overlays: &mut BTreeMap<String, Option<String>>,
    exact_overlays: &mut BTreeMap<String, Option<CodeExactFileState>>,
    planned_files: &mut Vec<CodeSessionLineageUndoPlanFile>,
    conflicts: &mut Vec<CodeSessionLineageUndoPlanConflict>,
) -> CommandResult<()> {
    let current =
        current_file_operation_state_for_session_lineage_plan(repo_root, exact_overlays, file)?;
    let exact_file = exact_session_lineage_planner_file(file);
    let plan = plan_file_operation_inverse_patch(&exact_file, &current);

    if plan.status == CodeFileOperationInversePatchPlanStatus::Conflicted {
        conflicts.extend(
            plan.conflicts.iter().map(|conflict| {
                session_lineage_conflict_from_file_operation(group, file, conflict)
            }),
        );
        return Ok(());
    }

    let current_hash = current
        .path_after
        .as_ref()
        .or(current.path_before.as_ref())
        .and_then(|state| state.content_hash.clone());
    let planned_hash = plan
        .actions
        .iter()
        .rev()
        .filter_map(|action| action.restore_state.as_ref())
        .find_map(|state| state.content_hash.clone());
    let inverse_hunk_ids = file
        .hunks
        .iter()
        .map(|hunk| hunk.hunk_id.clone())
        .collect::<Vec<_>>();

    planned_files.push(CodeSessionLineageUndoPlanFile {
        change_group_id: group.change_group_id.clone(),
        commit_id: group.commit_id.clone(),
        patch_file_id: file.patch_file_id.clone(),
        path_before: file.path_before.clone(),
        path_after: file.path_after.clone(),
        operation: file.operation,
        merge_policy: exact_file.merge_policy,
        current_hash,
        planned_hash,
        planned_content: None,
        file_actions: plan.actions.clone(),
        inverse_hunk_ids,
        preserved_dirty_overlay: false,
    });

    apply_exact_session_lineage_actions_to_overlay(
        repo_root,
        text_overlays,
        exact_overlays,
        &plan.actions,
    )?;
    Ok(())
}

fn current_text_for_session_lineage_plan(
    repo_root: &Path,
    text_overlays: &BTreeMap<String, Option<String>>,
    path: &str,
) -> CommandResult<Option<String>> {
    if let Some(content) = text_overlays.get(path) {
        return Ok(content.clone());
    }

    let absolute_path = absolute_history_plan_repo_path(repo_root, path)?;
    match fs::read(&absolute_path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|error| {
            CommandError::user_fixable(
                "code_session_lineage_undo_plan_text_decode_failed",
                format!(
                    "The current file `{path}` is not UTF-8 text, so the session rollback cannot be planned safely: {error}"
                ),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CommandError::retryable(
            "code_session_lineage_undo_plan_text_read_failed",
            format!("Xero could not read `{path}` before session rollback planning: {error}"),
        )),
    }
}

fn current_file_operation_state_for_session_lineage_plan(
    repo_root: &Path,
    exact_overlays: &BTreeMap<String, Option<CodeExactFileState>>,
    file: &CodePatchFileRecord,
) -> CommandResult<CodeFileOperationCurrentState> {
    let mut current = CodeFileOperationCurrentState::default();
    if let Some(path_before) = file.path_before.as_deref() {
        current.path_before =
            current_exact_state_for_session_lineage_plan(repo_root, exact_overlays, path_before)?;
    }
    if let Some(path_after) = file.path_after.as_deref() {
        if file.path_before.as_deref() == Some(path_after) {
            current.path_after = current.path_before.clone();
        } else {
            current.path_after = current_exact_state_for_session_lineage_plan(
                repo_root,
                exact_overlays,
                path_after,
            )?;
        }
    }
    Ok(current)
}

fn current_exact_state_for_session_lineage_plan(
    repo_root: &Path,
    exact_overlays: &BTreeMap<String, Option<CodeExactFileState>>,
    path: &str,
) -> CommandResult<Option<CodeExactFileState>> {
    if let Some(state) = exact_overlays.get(path) {
        return Ok(state.clone());
    }

    let absolute_path = absolute_history_plan_repo_path(repo_root, path)?;
    let metadata = match fs::symlink_metadata(&absolute_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CommandError::retryable(
                "code_session_lineage_undo_plan_state_read_failed",
                format!(
                    "Xero could not inspect `{path}` before session rollback planning: {error}"
                ),
            ));
        }
    };

    let file_type = metadata.file_type();
    if file_type.is_symlink() {
        let symlink_target = fs::read_link(&absolute_path).map_err(|error| {
            CommandError::retryable(
                "code_session_lineage_undo_plan_symlink_read_failed",
                format!(
                    "Xero could not read symlink target for `{path}` before session rollback planning: {error}"
                ),
            )
        })?;
        return Ok(Some(CodeExactFileState {
            kind: CodePatchFileKind::Symlink,
            content_hash: None,
            blob_id: None,
            size: Some(metadata.len()),
            mode: history_plan_file_mode(&metadata),
            symlink_target: Some(symlink_target.to_string_lossy().into_owned()),
        }));
    }

    if metadata.is_dir() {
        return Ok(Some(CodeExactFileState {
            kind: CodePatchFileKind::Directory,
            content_hash: None,
            blob_id: None,
            size: None,
            mode: history_plan_file_mode(&metadata),
            symlink_target: None,
        }));
    }

    if metadata.is_file() {
        let bytes = fs::read(&absolute_path).map_err(|error| {
            CommandError::retryable(
                "code_session_lineage_undo_plan_file_read_failed",
                format!("Xero could not read `{path}` before session rollback planning: {error}"),
            )
        })?;
        let hash = sha256_hex(&bytes);
        return Ok(Some(CodeExactFileState {
            kind: CodePatchFileKind::File,
            content_hash: Some(hash.clone()),
            blob_id: Some(hash),
            size: Some(bytes.len() as u64),
            mode: history_plan_file_mode(&metadata),
            symlink_target: None,
        }));
    }

    Err(CommandError::user_fixable(
        "code_session_lineage_undo_plan_path_unsupported",
        format!("The current path `{path}` is neither a file, directory, nor symlink."),
    ))
}

fn apply_exact_session_lineage_actions_to_overlay(
    repo_root: &Path,
    text_overlays: &mut BTreeMap<String, Option<String>>,
    exact_overlays: &mut BTreeMap<String, Option<CodeExactFileState>>,
    actions: &[CodeFileOperationInverseAction],
) -> CommandResult<()> {
    for action in actions {
        match action.kind {
            CodeFileOperationInverseActionKind::RemovePath => {
                exact_overlays.insert(action.target_path.clone(), None);
                text_overlays.insert(action.target_path.clone(), None);
            }
            CodeFileOperationInverseActionKind::RenamePath => {
                let source_path = action.source_path.as_deref().ok_or_else(|| {
                    CommandError::system_fault(
                        "code_session_lineage_undo_plan_action_invalid",
                        "A rename rollback planning action was missing its source path.",
                    )
                })?;
                let restored_state = match action.restore_state.clone() {
                    Some(state) => Some(state),
                    None => current_exact_state_for_session_lineage_plan(
                        repo_root,
                        exact_overlays,
                        source_path,
                    )?,
                };
                exact_overlays.insert(source_path.to_string(), None);
                text_overlays.insert(source_path.to_string(), None);
                exact_overlays.insert(action.target_path.clone(), restored_state);
                text_overlays.remove(&action.target_path);
            }
            CodeFileOperationInverseActionKind::RestoreMode
            | CodeFileOperationInverseActionKind::RestorePath
            | CodeFileOperationInverseActionKind::RestoreSymlink => {
                exact_overlays.insert(action.target_path.clone(), action.restore_state.clone());
                text_overlays.remove(&action.target_path);
            }
        }
    }
    Ok(())
}

fn exact_session_lineage_planner_file(file: &CodePatchFileRecord) -> CodePatchFileRecord {
    if file.operation == CodePatchFileOperation::Modify {
        return file.clone();
    }
    let mut exact_file = file.clone();
    exact_file.merge_policy = CodePatchMergePolicy::Exact;
    exact_file
}

fn exact_state_for_planned_text(
    content: &str,
    current_state: Option<&CodeExactFileState>,
) -> CodeExactFileState {
    let hash = sha256_hex(content.as_bytes());
    CodeExactFileState {
        kind: CodePatchFileKind::File,
        content_hash: Some(hash.clone()),
        blob_id: Some(hash),
        size: Some(content.len() as u64),
        mode: current_state.and_then(|state| state.mode),
        symlink_target: None,
    }
}

fn current_hash_is_preserved_overlay(
    current_hash: Option<&str>,
    file: &CodePatchFileRecord,
) -> bool {
    let Some(current_hash) = current_hash else {
        return false;
    };
    Some(current_hash) != file.result_hash.as_deref()
        && Some(current_hash) != file.base_hash.as_deref()
}

fn session_lineage_conflict_from_text(
    group: &CodeSessionLineageUndoPlanChangeGroup,
    file: &CodePatchFileRecord,
    conflict: &CodeTextInversePatchConflict,
) -> CodeSessionLineageUndoPlanConflict {
    CodeSessionLineageUndoPlanConflict {
        change_group_id: group.change_group_id.clone(),
        commit_id: group.commit_id.clone(),
        patch_file_id: file.patch_file_id.clone(),
        path: conflict.path.clone(),
        kind: match conflict.kind {
            CodeTextInversePatchConflictKind::TextOverlap => {
                CodeSessionLineageUndoPlanConflictKind::TextOverlap
            }
            CodeTextInversePatchConflictKind::FileMissing => {
                CodeSessionLineageUndoPlanConflictKind::FileMissing
            }
            CodeTextInversePatchConflictKind::UnsupportedOperation => {
                CodeSessionLineageUndoPlanConflictKind::UnsupportedOperation
            }
        },
        message: conflict.message.clone(),
        base_hash: conflict.base_hash.clone(),
        selected_hash: conflict.selected_hash.clone(),
        current_hash: conflict.current_hash.clone(),
        hunk_ids: conflict.hunk_ids.clone(),
    }
}

fn session_lineage_conflict_from_file_operation(
    group: &CodeSessionLineageUndoPlanChangeGroup,
    file: &CodePatchFileRecord,
    conflict: &CodeFileOperationInverseConflict,
) -> CodeSessionLineageUndoPlanConflict {
    CodeSessionLineageUndoPlanConflict {
        change_group_id: group.change_group_id.clone(),
        commit_id: group.commit_id.clone(),
        patch_file_id: file.patch_file_id.clone(),
        path: conflict.path.clone(),
        kind: match conflict.kind {
            CodeFileOperationInverseConflictKind::CurrentStateMismatch => {
                CodeSessionLineageUndoPlanConflictKind::CurrentStateMismatch
            }
            CodeFileOperationInverseConflictKind::PathAlreadyExists => {
                CodeSessionLineageUndoPlanConflictKind::PathAlreadyExists
            }
            CodeFileOperationInverseConflictKind::PathMissing => {
                CodeSessionLineageUndoPlanConflictKind::PathMissing
            }
            CodeFileOperationInverseConflictKind::UnsupportedOperation => {
                CodeSessionLineageUndoPlanConflictKind::UnsupportedOperation
            }
        },
        message: conflict.message.clone(),
        base_hash: file.base_hash.clone(),
        selected_hash: conflict
            .expected_state
            .as_ref()
            .and_then(|state| state.content_hash.clone()),
        current_hash: conflict
            .current_state
            .as_ref()
            .and_then(|state| state.content_hash.clone()),
        hunk_ids: file.hunks.iter().map(|hunk| hunk.hunk_id.clone()).collect(),
    }
}

fn absolute_history_plan_repo_path(
    repo_root: &Path,
    relative_path: &str,
) -> CommandResult<PathBuf> {
    let Some(relative_path) = safe_history_plan_relative_path(relative_path) else {
        return Err(CommandError::invalid_request("path"));
    };
    Ok(repo_root.join(relative_path))
}

fn safe_history_plan_relative_path(value: &str) -> Option<PathBuf> {
    let path = Path::new(value);
    if path.is_absolute() {
        return None;
    }

    let mut sanitized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => sanitized.push(segment),
            Component::CurDir => {}
            _ => return None,
        }
    }
    (!sanitized.as_os_str().is_empty()).then_some(sanitized)
}

fn normalize_code_workspace_epoch_paths(paths: &[String]) -> CommandResult<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for path in paths {
        let Some(relative_path) = safe_history_plan_relative_path(path) else {
            return Err(CommandError::new(
                "agent_file_path_invalid",
                CommandErrorClass::PolicyDenied,
                format!(
                    "Xero refused to modify `{path}` because it is not a safe repo-relative path."
                ),
                false,
            ));
        };
        normalized.insert(relative_path.to_string_lossy().replace('\\', "/"));
    }
    Ok(normalized.into_iter().collect())
}

fn history_plan_file_mode(metadata: &fs::Metadata) -> Option<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Some(metadata.permissions().mode() & 0o7777)
    }

    #[cfg(not(unix))]
    {
        let _ = metadata;
        None
    }
}

#[derive(Debug)]
struct CompletedChangeGroupMetadata {
    agent_session_id: String,
    run_id: String,
}

#[derive(Debug, Clone)]
struct BoundaryRunRow {
    run_id: String,
    agent_session_id: String,
    parent_run_id: Option<String>,
}

#[derive(Debug, Clone)]
struct CodeSessionBoundarySource {
    source_kind: CodeSessionBoundarySourceKind,
    source_id: String,
    run_id: String,
    created_at: String,
    change_group_id: Option<String>,
    commit_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedCodeSessionBoundaryReference {
    ChangeGroup(String),
    Checkpoint(i64),
    Event(i64),
    FileChange(i64),
    Message(i64),
    Run(String),
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

fn resolve_code_session_boundary_change_group(
    connection: &Connection,
    database_path: &Path,
    request: &ResolveCodeSessionBoundaryRequest,
    change_group_id: &str,
) -> CommandResult<ResolvedCodeSessionBoundary> {
    validate_required_text(change_group_id, "changeGroupId")?;
    let commit = read_latest_code_commit_for_change_group_from_connection(
        connection,
        &request.project_id,
        change_group_id,
        database_path,
    )?
    .ok_or_else(|| {
        no_code_commit_for_boundary_error(
            CodeSessionBoundarySourceKind::ChangeGroup,
            change_group_id,
        )
    })?;
    let lineage =
        resolve_code_session_boundary_lineage(connection, database_path, request, &commit.run_id)?;
    ensure_commit_in_lineage(&commit, &lineage)?;

    Ok(ResolvedCodeSessionBoundary {
        project_id: request.project_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        target_kind: request.target_kind,
        target_id: request.target_id.clone(),
        boundary_id: request.boundary_id.clone(),
        source_kind: CodeSessionBoundarySourceKind::ChangeGroup,
        source_id: change_group_id.into(),
        boundary_run_id: commit.run_id.clone(),
        boundary_created_at: Some(commit.completed_at.clone()),
        boundary_change_group_id: Some(commit.change_group_id.clone()),
        commit,
        lineage,
    })
}

fn resolve_code_session_boundary_source(
    connection: &Connection,
    database_path: &Path,
    request: &ResolveCodeSessionBoundaryRequest,
    source: CodeSessionBoundarySource,
) -> CommandResult<ResolvedCodeSessionBoundary> {
    let lineage =
        resolve_code_session_boundary_lineage(connection, database_path, request, &source.run_id)?;
    let commit = if let Some(commit_id) = source.commit_id.as_deref() {
        read_code_commit_from_connection(connection, &request.project_id, commit_id, database_path)?
            .ok_or_else(|| {
                no_code_commit_for_boundary_error(source.source_kind, source.source_id.as_str())
            })?
    } else if let Some(change_group_id) = source.change_group_id.as_deref() {
        read_latest_code_commit_for_change_group_from_connection(
            connection,
            &request.project_id,
            change_group_id,
            database_path,
        )?
        .ok_or_else(|| {
            no_code_commit_for_boundary_error(source.source_kind, source.source_id.as_str())
        })?
    } else {
        read_latest_code_commit_for_lineage_from_connection(
            connection,
            &request.project_id,
            &lineage,
            Some(source.created_at.as_str()),
            database_path,
        )?
        .ok_or_else(|| {
            no_code_commit_for_boundary_error(source.source_kind, source.source_id.as_str())
        })?
    };
    ensure_commit_in_lineage(&commit, &lineage)?;
    if commit.completed_at > source.created_at
        && source.commit_id.is_none()
        && source.change_group_id.is_none()
    {
        return Err(no_code_commit_for_boundary_error(
            source.source_kind,
            source.source_id.as_str(),
        ));
    }

    Ok(ResolvedCodeSessionBoundary {
        project_id: request.project_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        target_kind: request.target_kind,
        target_id: request.target_id.clone(),
        boundary_id: request.boundary_id.clone(),
        source_kind: source.source_kind,
        source_id: source.source_id,
        boundary_run_id: source.run_id,
        boundary_created_at: Some(source.created_at),
        boundary_change_group_id: Some(commit.change_group_id.clone()),
        commit,
        lineage,
    })
}

fn resolve_code_session_boundary_run(
    connection: &Connection,
    database_path: &Path,
    request: &ResolveCodeSessionBoundaryRequest,
    boundary_run_id: &str,
) -> CommandResult<ResolvedCodeSessionBoundary> {
    let lineage =
        resolve_code_session_boundary_lineage(connection, database_path, request, boundary_run_id)?;
    let commit = read_latest_code_commit_for_lineage_from_connection(
        connection,
        &request.project_id,
        &lineage,
        None,
        database_path,
    )?
    .ok_or_else(|| {
        no_code_commit_for_boundary_error(CodeSessionBoundarySourceKind::Run, boundary_run_id)
    })?;
    ensure_commit_in_lineage(&commit, &lineage)?;

    Ok(ResolvedCodeSessionBoundary {
        project_id: request.project_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        target_kind: request.target_kind,
        target_id: request.target_id.clone(),
        boundary_id: request.boundary_id.clone(),
        source_kind: CodeSessionBoundarySourceKind::Run,
        source_id: boundary_run_id.into(),
        boundary_run_id: boundary_run_id.into(),
        boundary_created_at: None,
        boundary_change_group_id: Some(commit.change_group_id.clone()),
        commit,
        lineage,
    })
}

fn resolve_code_session_boundary_lineage(
    connection: &Connection,
    database_path: &Path,
    request: &ResolveCodeSessionBoundaryRequest,
    boundary_run_id: &str,
) -> CommandResult<CodeSessionBoundaryLineageRecord> {
    let boundary_run = read_boundary_run_row(
        connection,
        database_path,
        &request.project_id,
        boundary_run_id,
    )?;
    if boundary_run.agent_session_id != request.agent_session_id {
        return Err(CommandError::user_fixable(
            "code_session_boundary_session_mismatch",
            format!(
                "Boundary run `{boundary_run_id}` belongs to agent session `{}`, not `{}`.",
                boundary_run.agent_session_id, request.agent_session_id
            ),
        ));
    }

    let (target_run_id, included_run_ids) = match request.target_kind {
        CodeSessionBoundaryTargetKind::SessionBoundary => (
            None,
            read_session_scope_run_ids(
                connection,
                database_path,
                &request.project_id,
                &request.agent_session_id,
            )?,
        ),
        CodeSessionBoundaryTargetKind::RunBoundary => {
            let target_run_id = request.run_id.as_deref().ok_or_else(|| {
                CommandError::user_fixable(
                    "code_session_boundary_run_required",
                    "Run-boundary session rollback targets must include a run id.",
                )
            })?;
            let target_run = read_boundary_run_row(
                connection,
                database_path,
                &request.project_id,
                target_run_id,
            )?;
            if target_run.agent_session_id != request.agent_session_id {
                return Err(CommandError::user_fixable(
                    "code_session_boundary_session_mismatch",
                    format!(
                        "Target run `{target_run_id}` belongs to agent session `{}`, not `{}`.",
                        target_run.agent_session_id, request.agent_session_id
                    ),
                ));
            }
            (
                Some(target_run_id.to_string()),
                read_run_scope_run_ids(
                    connection,
                    database_path,
                    &request.project_id,
                    &target_run,
                )?,
            )
        }
    };

    if !included_run_ids
        .iter()
        .any(|run_id| run_id == boundary_run_id)
    {
        return Err(CommandError::user_fixable(
            "code_session_boundary_run_mismatch",
            format!(
                "Boundary run `{boundary_run_id}` is outside the selected rollback target lineage."
            ),
        ));
    }

    let root_run_id = boundary_run
        .parent_run_id
        .clone()
        .unwrap_or_else(|| boundary_run.run_id.clone());
    Ok(CodeSessionBoundaryLineageRecord {
        project_id: request.project_id.clone(),
        agent_session_id: request.agent_session_id.clone(),
        target_kind: request.target_kind,
        target_run_id,
        boundary_run_id: boundary_run.run_id,
        root_run_id,
        parent_run_id: boundary_run.parent_run_id,
        included_run_ids,
    })
}

fn ensure_commit_in_lineage(
    commit: &CodeCommitRecord,
    lineage: &CodeSessionBoundaryLineageRecord,
) -> CommandResult<()> {
    if commit.agent_session_id != lineage.agent_session_id {
        return Err(CommandError::user_fixable(
            "code_session_boundary_session_mismatch",
            format!(
                "Code commit `{}` belongs to agent session `{}`, not `{}`.",
                commit.commit_id, commit.agent_session_id, lineage.agent_session_id
            ),
        ));
    }
    if !lineage
        .included_run_ids
        .iter()
        .any(|run_id| run_id == &commit.run_id)
    {
        return Err(CommandError::user_fixable(
            "code_session_boundary_run_mismatch",
            format!(
                "Code commit `{}` belongs to run `{}`, which is outside the selected rollback target lineage.",
                commit.commit_id, commit.run_id
            ),
        ));
    }
    Ok(())
}

fn read_checkpoint_boundary_source(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    checkpoint_id: i64,
) -> CommandResult<CodeSessionBoundarySource> {
    let (run_id, payload_json, created_at): (String, Option<String>, String) = connection
        .query_row(
            r#"
            SELECT run_id, payload_json, created_at
            FROM agent_checkpoints
            WHERE project_id = ?1
              AND id = ?2
            "#,
            params![project_id, checkpoint_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_checkpoint_read_failed", error)
        })?
        .ok_or_else(|| {
            boundary_not_found_error(CodeSessionBoundarySourceKind::Checkpoint, checkpoint_id)
        })?;
    let payload = parse_optional_boundary_payload(payload_json.as_deref(), checkpoint_id)?;
    Ok(CodeSessionBoundarySource {
        source_kind: CodeSessionBoundarySourceKind::Checkpoint,
        source_id: checkpoint_id.to_string(),
        run_id,
        created_at,
        change_group_id: boundary_payload_string(
            &payload,
            "codeChangeGroupId",
            "xero.code_change_group_id",
        ),
        commit_id: boundary_payload_string(&payload, "codeCommitId", "xero.code_commit_id"),
    })
}

fn read_event_boundary_source(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    event_id: i64,
) -> CommandResult<CodeSessionBoundarySource> {
    let (run_id, payload_json, created_at): (String, String, String) = connection
        .query_row(
            r#"
            SELECT run_id, payload_json, created_at
            FROM agent_events
            WHERE project_id = ?1
              AND id = ?2
            "#,
            params![project_id, event_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_event_read_failed", error)
        })?
        .ok_or_else(|| boundary_not_found_error(CodeSessionBoundarySourceKind::Event, event_id))?;
    let payload = parse_boundary_payload(&payload_json, event_id)?;
    let commit_id = read_code_commit_id_for_runtime_event_from_connection(
        connection,
        project_id,
        event_id,
        database_path,
    )?
    .or_else(|| boundary_payload_string(&payload, "codeCommitId", "xero.code_commit_id"));
    Ok(CodeSessionBoundarySource {
        source_kind: CodeSessionBoundarySourceKind::Event,
        source_id: event_id.to_string(),
        run_id,
        created_at,
        change_group_id: boundary_payload_string(
            &payload,
            "codeChangeGroupId",
            "xero.code_change_group_id",
        ),
        commit_id,
    })
}

fn read_file_change_boundary_source(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    file_change_id: i64,
) -> CommandResult<CodeSessionBoundarySource> {
    let (run_id, change_group_id, created_at): (String, Option<String>, String) = connection
        .query_row(
            r#"
            SELECT run_id, change_group_id, created_at
            FROM agent_file_changes
            WHERE project_id = ?1
              AND id = ?2
            "#,
            params![project_id, file_change_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_file_change_read_failed", error)
        })?
        .ok_or_else(|| {
            boundary_not_found_error(CodeSessionBoundarySourceKind::FileChange, file_change_id)
        })?;
    Ok(CodeSessionBoundarySource {
        source_kind: CodeSessionBoundarySourceKind::FileChange,
        source_id: file_change_id.to_string(),
        run_id,
        created_at,
        change_group_id,
        commit_id: None,
    })
}

fn read_message_boundary_source(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    message_id: i64,
) -> CommandResult<CodeSessionBoundarySource> {
    let (run_id, created_at): (String, String) = connection
        .query_row(
            r#"
            SELECT run_id, created_at
            FROM agent_messages
            WHERE project_id = ?1
              AND id = ?2
            "#,
            params![project_id, message_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_message_read_failed", error)
        })?
        .ok_or_else(|| {
            boundary_not_found_error(CodeSessionBoundarySourceKind::Message, message_id)
        })?;
    Ok(CodeSessionBoundarySource {
        source_kind: CodeSessionBoundarySourceKind::Message,
        source_id: message_id.to_string(),
        run_id,
        created_at,
        change_group_id: None,
        commit_id: None,
    })
}

fn read_latest_code_commit_for_change_group_from_connection(
    connection: &Connection,
    project_id: &str,
    change_group_id: &str,
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
              AND change_group_id = ?2
            ORDER BY workspace_epoch DESC, completed_at DESC, commit_id DESC
            LIMIT 1
            "#,
            params![project_id, change_group_id],
            raw_code_commit_from_row,
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_commit_lookup_failed", error)
        })?;
    raw.map(code_commit_from_raw).transpose()
}

fn read_code_commits_after_boundary_from_connection(
    connection: &Connection,
    project_id: &str,
    boundary_workspace_epoch: u64,
    limit: usize,
    database_path: &Path,
) -> CommandResult<Vec<CodeCommitRecord>> {
    if limit == 0 {
        return Err(code_session_lineage_undo_plan_budget_exceeded(
            "commitsAfterBoundary",
            limit,
        ));
    }
    let boundary_workspace_epoch = epoch_to_sql(boundary_workspace_epoch)?;
    let query_limit = usize_to_sql(limit.saturating_add(1), "commitScanLimit")?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                code_commits.project_id,
                code_commits.commit_id,
                code_commits.parent_commit_id,
                code_commits.tree_id,
                code_commits.parent_tree_id,
                code_commits.patchset_id,
                code_commits.change_group_id,
                code_commits.history_operation_id,
                code_commits.agent_session_id,
                code_commits.run_id,
                code_commits.tool_call_id,
                code_commits.runtime_event_id,
                code_commits.conversation_sequence,
                code_commits.commit_kind,
                code_commits.summary_label,
                code_commits.workspace_epoch,
                code_commits.created_at,
                code_commits.completed_at
            FROM code_commits
            JOIN code_change_groups
              ON code_change_groups.project_id = code_commits.project_id
             AND code_change_groups.change_group_id = code_commits.change_group_id
            WHERE code_commits.project_id = ?1
              AND code_commits.workspace_epoch > ?2
              AND code_change_groups.status = 'completed'
            ORDER BY code_commits.workspace_epoch DESC,
                     code_commits.completed_at DESC,
                     code_commits.commit_id DESC
            LIMIT ?3
            "#,
        )
        .map_err(|error| {
            map_code_history_storage_error(
                database_path,
                "code_session_lineage_undo_plan_prepare_failed",
                error,
            )
        })?;
    let rows = statement
        .query_map(
            params![project_id, boundary_workspace_epoch, query_limit],
            raw_code_commit_from_row,
        )
        .map_err(|error| {
            map_code_history_storage_error(
                database_path,
                "code_session_lineage_undo_plan_read_failed",
                error,
            )
        })?;
    let commits = rows
        .map(|row| {
            row.map_err(|error| {
                map_code_history_storage_error(
                    database_path,
                    "code_session_lineage_undo_plan_decode_failed",
                    error,
                )
            })
            .and_then(code_commit_from_raw)
        })
        .collect::<CommandResult<Vec<_>>>()?;
    if commits.len() > limit {
        return Err(code_session_lineage_undo_plan_budget_exceeded(
            "commitsAfterBoundary",
            limit,
        ));
    }
    Ok(commits)
}

fn read_code_commit_id_for_runtime_event_from_connection(
    connection: &Connection,
    project_id: &str,
    runtime_event_id: i64,
    database_path: &Path,
) -> CommandResult<Option<String>> {
    connection
        .query_row(
            r#"
            SELECT commit_id
            FROM code_commits
            WHERE project_id = ?1
              AND runtime_event_id = ?2
            ORDER BY workspace_epoch DESC, completed_at DESC, commit_id DESC
            LIMIT 1
            "#,
            params![project_id, runtime_event_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "code_commit_event_lookup_failed", error)
        })
}

fn code_commit_is_default_lineage_undo_target(
    commit: &CodeCommitRecord,
    lineage: &CodeSessionBoundaryLineageRecord,
) -> bool {
    commit.agent_session_id == lineage.agent_session_id
        && lineage
            .included_run_ids
            .iter()
            .any(|run_id| run_id == &commit.run_id)
        && commit.commit_kind == CodeHistoryCommitKind::ChangeGroup
}

fn code_session_lineage_undo_plan_exclusion_reason(
    commit: &CodeCommitRecord,
    lineage: &CodeSessionBoundaryLineageRecord,
) -> Option<CodeSessionLineageUndoPlanExclusionReason> {
    if commit.agent_session_id != lineage.agent_session_id {
        return Some(CodeSessionLineageUndoPlanExclusionReason::SiblingSession);
    }
    if !lineage
        .included_run_ids
        .iter()
        .any(|run_id| run_id == &commit.run_id)
    {
        return Some(CodeSessionLineageUndoPlanExclusionReason::OutsideRunLineage);
    }
    if commit.commit_kind != CodeHistoryCommitKind::ChangeGroup {
        return Some(CodeSessionLineageUndoPlanExclusionReason::UserOrRecoveredChange);
    }
    None
}

fn read_latest_code_commit_for_lineage_from_connection(
    connection: &Connection,
    project_id: &str,
    lineage: &CodeSessionBoundaryLineageRecord,
    completed_at_or_before: Option<&str>,
    database_path: &Path,
) -> CommandResult<Option<CodeCommitRecord>> {
    match lineage.target_kind {
        CodeSessionBoundaryTargetKind::SessionBoundary => {
            read_latest_code_commit_for_session_from_connection(
                connection,
                project_id,
                &lineage.agent_session_id,
                completed_at_or_before,
                database_path,
            )
        }
        CodeSessionBoundaryTargetKind::RunBoundary => {
            let target_run_id = lineage.target_run_id.as_deref().ok_or_else(|| {
                CommandError::system_fault(
                    "code_session_boundary_lineage_invalid",
                    "Run-boundary lineage was missing its target run id.",
                )
            })?;
            read_latest_code_commit_for_run_scope_from_connection(
                connection,
                project_id,
                &lineage.agent_session_id,
                target_run_id,
                lineage.included_run_ids.len() > 1,
                completed_at_or_before,
                database_path,
            )
        }
    }
}

fn read_latest_code_commit_for_session_from_connection(
    connection: &Connection,
    project_id: &str,
    agent_session_id: &str,
    completed_at_or_before: Option<&str>,
    database_path: &Path,
) -> CommandResult<Option<CodeCommitRecord>> {
    let raw = if let Some(cutoff) = completed_at_or_before {
        connection
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
                  AND agent_session_id = ?2
                  AND completed_at <= ?3
                ORDER BY workspace_epoch DESC, completed_at DESC, commit_id DESC
                LIMIT 1
                "#,
                params![project_id, agent_session_id, cutoff],
                raw_code_commit_from_row,
            )
            .optional()
    } else {
        connection
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
                  AND agent_session_id = ?2
                ORDER BY workspace_epoch DESC, completed_at DESC, commit_id DESC
                LIMIT 1
                "#,
                params![project_id, agent_session_id],
                raw_code_commit_from_row,
            )
            .optional()
    }
    .map_err(|error| {
        map_code_history_storage_error(database_path, "code_commit_lineage_lookup_failed", error)
    })?;
    raw.map(code_commit_from_raw).transpose()
}

fn read_latest_code_commit_for_run_scope_from_connection(
    connection: &Connection,
    project_id: &str,
    agent_session_id: &str,
    target_run_id: &str,
    include_child_runs: bool,
    completed_at_or_before: Option<&str>,
    database_path: &Path,
) -> CommandResult<Option<CodeCommitRecord>> {
    let child_filter = if include_child_runs {
        r#"
            OR code_commits.run_id IN (
                SELECT run_id
                FROM agent_runs
                WHERE project_id = ?1
                  AND agent_session_id = ?2
                  AND parent_run_id = ?3
            )
        "#
    } else {
        ""
    };
    let cutoff_filter = if completed_at_or_before.is_some() {
        "AND code_commits.completed_at <= ?4"
    } else {
        ""
    };
    let sql = format!(
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
          AND agent_session_id = ?2
          AND (code_commits.run_id = ?3 {child_filter})
          {cutoff_filter}
        ORDER BY workspace_epoch DESC, completed_at DESC, commit_id DESC
        LIMIT 1
        "#
    );
    let raw = if let Some(cutoff) = completed_at_or_before {
        connection
            .query_row(
                &sql,
                params![project_id, agent_session_id, target_run_id, cutoff],
                raw_code_commit_from_row,
            )
            .optional()
    } else {
        connection
            .query_row(
                &sql,
                params![project_id, agent_session_id, target_run_id],
                raw_code_commit_from_row,
            )
            .optional()
    }
    .map_err(|error| {
        map_code_history_storage_error(database_path, "code_commit_lineage_lookup_failed", error)
    })?;
    raw.map(code_commit_from_raw).transpose()
}

fn read_boundary_run_row(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    run_id: &str,
) -> CommandResult<BoundaryRunRow> {
    connection
        .query_row(
            r#"
            SELECT run_id, agent_session_id, parent_run_id
            FROM agent_runs
            WHERE project_id = ?1
              AND run_id = ?2
            "#,
            params![project_id, run_id],
            |row| {
                Ok(BoundaryRunRow {
                    run_id: row.get(0)?,
                    agent_session_id: row.get(1)?,
                    parent_run_id: row.get(2)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_run_read_failed", error)
        })?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "code_session_boundary_run_missing",
                format!("Xero could not find run `{run_id}` while resolving a code boundary."),
            )
        })
}

fn read_session_scope_run_ids(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT run_id
            FROM agent_runs
            WHERE project_id = ?1
              AND agent_session_id = ?2
            ORDER BY started_at ASC, run_id ASC
            "#,
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_runs_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(params![project_id, agent_session_id], |row| row.get(0))
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_runs_read_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_code_history_storage_error(database_path, "agent_runs_decode_failed", error)
    })
}

fn read_run_scope_run_ids(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    target_run: &BoundaryRunRow,
) -> CommandResult<Vec<String>> {
    if target_run.parent_run_id.is_some() {
        return Ok(vec![target_run.run_id.clone()]);
    }

    let mut statement = connection
        .prepare(
            r#"
            SELECT run_id
            FROM agent_runs
            WHERE project_id = ?1
              AND agent_session_id = ?2
              AND (run_id = ?3 OR parent_run_id = ?3)
            ORDER BY CASE WHEN run_id = ?3 THEN 0 ELSE 1 END, started_at ASC, run_id ASC
            "#,
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_run_scope_prepare_failed", error)
        })?;
    let rows = statement
        .query_map(
            params![
                project_id,
                target_run.agent_session_id.as_str(),
                target_run.run_id.as_str()
            ],
            |row| row.get(0),
        )
        .map_err(|error| {
            map_code_history_storage_error(database_path, "agent_run_scope_read_failed", error)
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        map_code_history_storage_error(database_path, "agent_run_scope_decode_failed", error)
    })
}

fn validate_resolve_code_session_boundary_request(
    request: &ResolveCodeSessionBoundaryRequest,
) -> CommandResult<()> {
    validate_required_text(&request.project_id, "projectId")?;
    validate_required_text(&request.agent_session_id, "agentSessionId")?;
    validate_required_text(&request.target_id, "targetId")?;
    validate_required_text(&request.boundary_id, "boundaryId")?;
    if let Some(run_id) = request.run_id.as_deref() {
        validate_required_text(run_id, "runId")?;
    }
    if let Some(change_group_id) = request.change_group_id.as_deref() {
        validate_required_text(change_group_id, "changeGroupId")?;
    }
    if request.target_kind == CodeSessionBoundaryTargetKind::RunBoundary && request.run_id.is_none()
    {
        return Err(CommandError::user_fixable(
            "code_session_boundary_run_required",
            "Run-boundary session rollback targets must include a run id.",
        ));
    }
    Ok(())
}

fn validate_build_code_session_lineage_undo_plan_request(
    request: &BuildCodeSessionLineageUndoPlanRequest,
) -> CommandResult<()> {
    validate_resolve_code_session_boundary_request(&request.boundary)?;
    let _ =
        normalize_explicit_plan_change_group_ids(&request.explicitly_selected_change_group_ids)?;
    Ok(())
}

fn normalize_explicit_plan_change_group_ids(
    change_group_ids: &[String],
) -> CommandResult<Vec<String>> {
    let mut normalized = Vec::with_capacity(change_group_ids.len());
    let mut seen = BTreeSet::new();
    for change_group_id in change_group_ids {
        validate_required_text(change_group_id, "explicitlySelectedChangeGroupIds[]")?;
        if !seen.insert(change_group_id.clone()) {
            return Err(CommandError::user_fixable(
                "code_session_lineage_undo_plan_duplicate_explicit_target",
                format!("Change group `{change_group_id}` was explicitly selected more than once."),
            ));
        }
        normalized.push(change_group_id.clone());
    }
    Ok(normalized)
}

fn parse_code_session_boundary_reference(
    request: &ResolveCodeSessionBoundaryRequest,
) -> CommandResult<ParsedCodeSessionBoundaryReference> {
    if let Some(reference) = parse_code_session_boundary_reference_value(&request.boundary_id)? {
        return Ok(reference);
    }
    if request.target_id != request.boundary_id {
        if let Some(reference) = parse_code_session_boundary_reference_value(&request.target_id)? {
            return Ok(reference);
        }
    }
    if request.target_kind == CodeSessionBoundaryTargetKind::RunBoundary {
        let run_id = request.run_id.as_deref().ok_or_else(|| {
            CommandError::user_fixable(
                "code_session_boundary_run_required",
                "Run-boundary session rollback targets must include a run id.",
            )
        })?;
        if request.boundary_id == run_id || request.target_id == run_id {
            return Ok(ParsedCodeSessionBoundaryReference::Run(run_id.to_string()));
        }
    }
    Err(CommandError::user_fixable(
        "code_session_boundary_unsupported",
        "Code session rollback boundaries must reference a change group, file change, event, message, checkpoint, or run.",
    ))
}

fn parse_code_session_boundary_reference_value(
    value: &str,
) -> CommandResult<Option<ParsedCodeSessionBoundaryReference>> {
    let Some((prefix, raw_id)) = value.split_once(':') else {
        return Ok(None);
    };
    validate_required_text(raw_id, "boundaryId")?;
    match prefix {
        "change_group" | "changeGroup" => Ok(Some(
            ParsedCodeSessionBoundaryReference::ChangeGroup(raw_id.into()),
        )),
        "checkpoint" => Ok(Some(ParsedCodeSessionBoundaryReference::Checkpoint(
            parse_positive_boundary_row_id(raw_id, "checkpoint")?,
        ))),
        "event" => Ok(Some(ParsedCodeSessionBoundaryReference::Event(
            parse_positive_boundary_row_id(raw_id, "event")?,
        ))),
        "file_change" | "fileChange" => Ok(Some(ParsedCodeSessionBoundaryReference::FileChange(
            parse_positive_boundary_row_id(raw_id, "file_change")?,
        ))),
        "message" => Ok(Some(ParsedCodeSessionBoundaryReference::Message(
            parse_positive_boundary_row_id(raw_id, "message")?,
        ))),
        "run" => Ok(Some(ParsedCodeSessionBoundaryReference::Run(raw_id.into()))),
        _ => Ok(None),
    }
}

fn parse_positive_boundary_row_id(raw_id: &str, source: &str) -> CommandResult<i64> {
    let id = raw_id.parse::<i64>().map_err(|_| {
        CommandError::user_fixable(
            "code_session_boundary_invalid",
            format!("Boundary `{source}:{raw_id}` must use a positive integer id."),
        )
    })?;
    if id <= 0 {
        return Err(CommandError::user_fixable(
            "code_session_boundary_invalid",
            format!("Boundary `{source}:{raw_id}` must use a positive integer id."),
        ));
    }
    Ok(id)
}

fn parse_optional_boundary_payload(
    payload_json: Option<&str>,
    source_id: i64,
) -> CommandResult<Option<JsonValue>> {
    payload_json
        .map(|payload_json| parse_boundary_payload(payload_json, source_id))
        .transpose()
        .map(Option::flatten)
}

fn parse_boundary_payload(payload_json: &str, source_id: i64) -> CommandResult<Option<JsonValue>> {
    serde_json::from_str::<JsonValue>(payload_json)
        .map(Some)
        .map_err(|error| {
            CommandError::system_fault(
                "code_session_boundary_payload_decode_failed",
                format!(
                    "Xero could not decode code boundary payload `{source_id}` as JSON: {error}"
                ),
            )
        })
}

fn boundary_payload_string(
    payload: &Option<JsonValue>,
    primary_key: &str,
    legacy_key: &str,
) -> Option<String> {
    payload
        .as_ref()
        .and_then(|payload| payload.get(primary_key).or_else(|| payload.get(legacy_key)))
        .and_then(JsonValue::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn boundary_not_found_error(
    source_kind: CodeSessionBoundarySourceKind,
    source_id: i64,
) -> CommandError {
    CommandError::user_fixable(
        "code_session_boundary_not_found",
        format!(
            "Xero could not find {} boundary `{source_id}`.",
            code_session_boundary_source_label(source_kind)
        ),
    )
}

fn no_code_commit_for_boundary_error(
    source_kind: CodeSessionBoundarySourceKind,
    source_id: &str,
) -> CommandError {
    CommandError::user_fixable(
        "code_session_boundary_no_code_commit",
        format!(
            "The selected {} boundary `{source_id}` does not correspond to an internal code commit.",
            code_session_boundary_source_label(source_kind)
        ),
    )
}

fn code_session_lineage_undo_plan_budget_exceeded(budget_name: &str, limit: usize) -> CommandError {
    CommandError::user_fixable(
        "code_session_lineage_undo_plan_budget_exceeded",
        format!(
            "Session rollback planning exceeded the {budget_name} budget of {limit}. Narrow the rollback target or undo smaller change groups."
        ),
    )
}

fn code_session_boundary_source_label(source_kind: CodeSessionBoundarySourceKind) -> &'static str {
    match source_kind {
        CodeSessionBoundarySourceKind::ChangeGroup => "change group",
        CodeSessionBoundarySourceKind::Checkpoint => "checkpoint",
        CodeSessionBoundarySourceKind::Event => "event",
        CodeSessionBoundarySourceKind::FileChange => "file change",
        CodeSessionBoundarySourceKind::Message => "message",
        CodeSessionBoundarySourceKind::Run => "run",
    }
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

fn read_code_patchset_commit_for_commit_from_connection(
    connection: &Connection,
    project_id: &str,
    commit: CodeCommitRecord,
    database_path: &Path,
) -> CommandResult<CodePatchsetCommitRecord> {
    let patchset = read_code_patchset_from_connection(
        connection,
        project_id,
        &commit.patchset_id,
        database_path,
    )?
    .ok_or_else(|| missing_patchset_for_commit_error(&commit))?;
    let files = read_code_patch_files_from_connection(
        connection,
        project_id,
        &patchset.patchset_id,
        database_path,
    )?;

    Ok(CodePatchsetCommitRecord {
        commit,
        patchset,
        files,
    })
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

fn code_patch_availability_from_commit(
    record: &CodePatchsetCommitRecord,
) -> CodePatchAvailabilityRecord {
    let affected_paths = affected_paths_for_patch_records(&record.files);
    let available = !affected_paths.is_empty() && record.patchset.file_count > 0;
    let text_hunks = text_hunk_availability_for_patch_records(&record.files);
    CodePatchAvailabilityRecord {
        project_id: record.commit.project_id.clone(),
        target_change_group_id: record.commit.change_group_id.clone(),
        available,
        affected_paths,
        file_change_count: record.patchset.file_count,
        text_hunk_count: record.patchset.text_hunk_count,
        text_hunks,
        unavailable_reason: (!available)
            .then(|| "No replayable patch files were captured for this change group.".into()),
    }
}

fn text_hunk_availability_for_patch_records(
    files: &[CodePatchFileRecord],
) -> Vec<CodePatchTextHunkAvailabilityRecord> {
    files
        .iter()
        .flat_map(|file| {
            let file_path = patch_file_display_path(file);
            file.hunks
                .iter()
                .map(move |hunk| CodePatchTextHunkAvailabilityRecord {
                    hunk_id: hunk.hunk_id.clone(),
                    patch_file_id: Some(hunk.patch_file_id.clone()),
                    file_path: file_path.clone(),
                    hunk_index: hunk.hunk_index,
                    base_start_line: hunk.base_start_line,
                    base_line_count: hunk.base_line_count,
                    result_start_line: hunk.result_start_line,
                    result_line_count: hunk.result_line_count,
                })
        })
        .collect()
}

fn unavailable_code_change_group_history_metadata(
    project_id: &str,
    change_group_id: &str,
    reason: impl Into<String>,
) -> CodeChangeGroupHistoryMetadataRecord {
    CodeChangeGroupHistoryMetadataRecord {
        project_id: project_id.into(),
        target_change_group_id: change_group_id.into(),
        commit_id: None,
        workspace_epoch: None,
        patch_availability: CodePatchAvailabilityRecord {
            project_id: project_id.into(),
            target_change_group_id: change_group_id.into(),
            available: false,
            affected_paths: Vec::new(),
            file_change_count: 0,
            text_hunk_count: 0,
            text_hunks: Vec::new(),
            unavailable_reason: Some(reason.into()),
        },
    }
}

fn affected_paths_for_patch_records(files: &[CodePatchFileRecord]) -> Vec<String> {
    let mut paths = BTreeSet::new();
    for file in files {
        if let Some(path) = file.path_before.as_deref() {
            paths.insert(path.to_string());
        }
        if let Some(path) = file.path_after.as_deref() {
            paths.insert(path.to_string());
        }
    }
    paths.into_iter().collect()
}

#[derive(Debug, Clone)]
struct TextInverseConflictBase {
    path: String,
    base_hash: Option<String>,
    selected_hash: Option<String>,
    current_hash: Option<String>,
}

impl TextInverseConflictBase {
    fn conflict(
        &self,
        kind: CodeTextInversePatchConflictKind,
        message: impl Into<String>,
        hunk_ids: Vec<String>,
    ) -> CodeTextInversePatchConflict {
        CodeTextInversePatchConflict {
            path: self.path.clone(),
            kind,
            message: message.into(),
            base_hash: self.base_hash.clone(),
            selected_hash: self.selected_hash.clone(),
            current_hash: self.current_hash.clone(),
            hunk_ids,
        }
    }
}

fn clean_text_inverse_plan(
    path: String,
    current_hash: Option<String>,
    planned_content: String,
    inverse_hunks: Vec<CodeTextInverseHunkPlan>,
) -> CodeTextInversePatchPlan {
    CodeTextInversePatchPlan {
        path,
        status: CodeTextInversePatchPlanStatus::Clean,
        current_hash,
        planned_hash: Some(sha256_hex(planned_content.as_bytes())),
        planned_content: Some(planned_content),
        inverse_hunks,
        conflicts: Vec::new(),
    }
}

fn conflicted_text_inverse_plan(
    path: String,
    current_hash: Option<String>,
    conflicts: Vec<CodeTextInversePatchConflict>,
) -> CodeTextInversePatchPlan {
    CodeTextInversePatchPlan {
        path,
        status: CodeTextInversePatchPlanStatus::Conflicted,
        current_hash,
        planned_hash: None,
        planned_content: None,
        inverse_hunks: Vec::new(),
        conflicts,
    }
}

fn plan_create_inverse(
    file: &CodePatchFileRecord,
    current: &CodeFileOperationCurrentState,
) -> CodeFileOperationInversePatchPlan {
    if let Some(plan) = require_exact_file_operation(file) {
        return plan;
    }

    let Some(path) = file.path_after.clone() else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Create patches must include a result path.",
            )],
        );
    };
    let Some(expected_after) = exact_after_state(file) else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Create patches must include result file state.",
            )],
        );
    };

    if let Some(conflict) = exact_state_conflict(
        &path,
        current.path_after.as_ref(),
        &expected_after,
        "The current created path differs from the selected create result.",
    ) {
        return conflicted_file_operation_inverse_plan(file, vec![conflict]);
    }

    clean_file_operation_inverse_plan(
        file,
        vec![CodeFileOperationInverseAction {
            kind: CodeFileOperationInverseActionKind::RemovePath,
            source_path: None,
            target_path: path,
            restore_state: None,
        }],
    )
}

fn plan_delete_inverse(
    file: &CodePatchFileRecord,
    current: &CodeFileOperationCurrentState,
) -> CodeFileOperationInversePatchPlan {
    if let Some(plan) = require_exact_file_operation(file) {
        return plan;
    }

    let Some(path) = file.path_before.clone() else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Delete patches must include a base path.",
            )],
        );
    };
    let Some(before_state) = exact_before_state(file) else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Delete patches must include base file state.",
            )],
        );
    };

    if let Some(current_state) = current.path_before.clone() {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![CodeFileOperationInverseConflict {
                path,
                kind: CodeFileOperationInverseConflictKind::PathAlreadyExists,
                message: "The deleted path currently exists, so restoring the selected delete would overwrite current work.".into(),
                expected_state: None,
                current_state: Some(current_state),
            }],
        );
    }

    clean_file_operation_inverse_plan(
        file,
        vec![CodeFileOperationInverseAction {
            kind: CodeFileOperationInverseActionKind::RestorePath,
            source_path: None,
            target_path: path,
            restore_state: Some(before_state),
        }],
    )
}

fn plan_rename_inverse(
    file: &CodePatchFileRecord,
    current: &CodeFileOperationCurrentState,
) -> CodeFileOperationInversePatchPlan {
    if let Some(plan) = require_exact_file_operation(file) {
        return plan;
    }

    let (Some(path_before), Some(path_after)) = (file.path_before.clone(), file.path_after.clone())
    else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Rename patches must include both base and result paths.",
            )],
        );
    };
    let Some(before_state) = exact_before_state(file) else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Rename patches must include base file state.",
            )],
        );
    };
    let Some(expected_after) = exact_after_state(file) else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Rename patches must include result file state.",
            )],
        );
    };

    let mut conflicts = Vec::new();
    if let Some(current_before) = current.path_before.clone() {
        conflicts.push(CodeFileOperationInverseConflict {
            path: path_before.clone(),
            kind: CodeFileOperationInverseConflictKind::PathAlreadyExists,
            message: "The original rename source path currently exists, so reversing the rename would overwrite current work.".into(),
            expected_state: None,
            current_state: Some(current_before),
        });
    }
    if let Some(conflict) = exact_state_conflict(
        &path_after,
        current.path_after.as_ref(),
        &expected_after,
        "The current rename target differs from the selected rename result.",
    ) {
        conflicts.push(conflict);
    }
    if !conflicts.is_empty() {
        return conflicted_file_operation_inverse_plan(file, conflicts);
    }

    clean_file_operation_inverse_plan(
        file,
        vec![CodeFileOperationInverseAction {
            kind: CodeFileOperationInverseActionKind::RenamePath,
            source_path: Some(path_after),
            target_path: path_before,
            restore_state: Some(before_state),
        }],
    )
}

fn plan_modify_inverse(
    file: &CodePatchFileRecord,
    current: &CodeFileOperationCurrentState,
) -> CodeFileOperationInversePatchPlan {
    if file.merge_policy != CodePatchMergePolicy::Exact {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Text modify patches must use the text inverse patch planner.",
            )],
        );
    }

    plan_restore_current_after_to_before(
        file,
        current,
        CodeFileOperationInverseActionKind::RestorePath,
        "The current file differs from the selected exact modify result.",
    )
}

fn plan_metadata_inverse(
    file: &CodePatchFileRecord,
    current: &CodeFileOperationCurrentState,
    action_kind: CodeFileOperationInverseActionKind,
    mismatch_message: &'static str,
) -> CodeFileOperationInversePatchPlan {
    if let Some(plan) = require_exact_file_operation(file) {
        return plan;
    }

    plan_restore_current_after_to_before(file, current, action_kind, mismatch_message)
}

fn plan_restore_current_after_to_before(
    file: &CodePatchFileRecord,
    current: &CodeFileOperationCurrentState,
    action_kind: CodeFileOperationInverseActionKind,
    mismatch_message: &'static str,
) -> CodeFileOperationInversePatchPlan {
    let Some(path) = file.path_after.clone() else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Patch files that restore current state to base state must include a result path.",
            )],
        );
    };
    let Some(before_state) = exact_before_state(file) else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Patch files that restore current state to base state must include base file state.",
            )],
        );
    };
    let Some(expected_after) = exact_after_state(file) else {
        return conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Patch files that restore current state to base state must include result file state.",
            )],
        );
    };

    if let Some(conflict) = exact_state_conflict(
        &path,
        current.path_after.as_ref(),
        &expected_after,
        mismatch_message,
    ) {
        return conflicted_file_operation_inverse_plan(file, vec![conflict]);
    }

    clean_file_operation_inverse_plan(
        file,
        vec![CodeFileOperationInverseAction {
            kind: action_kind,
            source_path: None,
            target_path: path,
            restore_state: Some(before_state),
        }],
    )
}

fn require_exact_file_operation(
    file: &CodePatchFileRecord,
) -> Option<CodeFileOperationInversePatchPlan> {
    (file.merge_policy != CodePatchMergePolicy::Exact).then(|| {
        conflicted_file_operation_inverse_plan(
            file,
            vec![unsupported_file_operation_conflict(
                file,
                "Only exact-state patch files can be planned by the file operation inverse planner.",
            )],
        )
    })
}

fn clean_file_operation_inverse_plan(
    file: &CodePatchFileRecord,
    actions: Vec<CodeFileOperationInverseAction>,
) -> CodeFileOperationInversePatchPlan {
    CodeFileOperationInversePatchPlan {
        path_before: file.path_before.clone(),
        path_after: file.path_after.clone(),
        operation: file.operation,
        status: CodeFileOperationInversePatchPlanStatus::Clean,
        actions,
        conflicts: Vec::new(),
    }
}

fn conflicted_file_operation_inverse_plan(
    file: &CodePatchFileRecord,
    conflicts: Vec<CodeFileOperationInverseConflict>,
) -> CodeFileOperationInversePatchPlan {
    CodeFileOperationInversePatchPlan {
        path_before: file.path_before.clone(),
        path_after: file.path_after.clone(),
        operation: file.operation,
        status: CodeFileOperationInversePatchPlanStatus::Conflicted,
        actions: Vec::new(),
        conflicts,
    }
}

fn exact_state_conflict(
    path: &str,
    current_state: Option<&CodeExactFileState>,
    expected_state: &CodeExactFileState,
    mismatch_message: &'static str,
) -> Option<CodeFileOperationInverseConflict> {
    match current_state {
        Some(current_state) if exact_state_matches(current_state, expected_state) => None,
        Some(current_state) => Some(CodeFileOperationInverseConflict {
            path: path.into(),
            kind: CodeFileOperationInverseConflictKind::CurrentStateMismatch,
            message: mismatch_message.into(),
            expected_state: Some(expected_state.clone()),
            current_state: Some(current_state.clone()),
        }),
        None => Some(CodeFileOperationInverseConflict {
            path: path.into(),
            kind: CodeFileOperationInverseConflictKind::PathMissing,
            message: "The current path is missing, so the selected file operation cannot be undone safely.".into(),
            expected_state: Some(expected_state.clone()),
            current_state: None,
        }),
    }
}

fn exact_state_matches(current: &CodeExactFileState, expected: &CodeExactFileState) -> bool {
    current.kind == expected.kind
        && optional_expected_matches(&current.content_hash, &expected.content_hash)
        && optional_expected_matches(&current.blob_id, &expected.blob_id)
        && optional_expected_matches(&current.size, &expected.size)
        && optional_expected_matches(&current.mode, &expected.mode)
        && optional_expected_matches(&current.symlink_target, &expected.symlink_target)
}

fn optional_expected_matches<T: PartialEq>(current: &Option<T>, expected: &Option<T>) -> bool {
    match expected.as_ref() {
        Some(expected) => current.as_ref() == Some(expected),
        None => true,
    }
}

fn exact_before_state(file: &CodePatchFileRecord) -> Option<CodeExactFileState> {
    file.before_file_kind.map(|kind| CodeExactFileState {
        kind,
        content_hash: file.base_hash.clone(),
        blob_id: file.base_blob_id.clone(),
        size: file.base_size,
        mode: file.base_mode,
        symlink_target: file.base_symlink_target.clone(),
    })
}

fn exact_after_state(file: &CodePatchFileRecord) -> Option<CodeExactFileState> {
    file.after_file_kind.map(|kind| CodeExactFileState {
        kind,
        content_hash: file.result_hash.clone(),
        blob_id: file.result_blob_id.clone(),
        size: file.result_size,
        mode: file.result_mode,
        symlink_target: file.result_symlink_target.clone(),
    })
}

fn unsupported_file_operation_conflict(
    file: &CodePatchFileRecord,
    message: impl Into<String>,
) -> CodeFileOperationInverseConflict {
    CodeFileOperationInverseConflict {
        path: patch_file_display_path(file),
        kind: CodeFileOperationInverseConflictKind::UnsupportedOperation,
        message: message.into(),
        expected_state: None,
        current_state: None,
    }
}

fn patch_file_display_path(file: &CodePatchFileRecord) -> String {
    file.path_after
        .as_deref()
        .or(file.path_before.as_deref())
        .unwrap_or("<unknown>")
        .to_string()
}

fn locate_inverse_hunk(current_lines: &[String], hunk: &CodePatchHunkRecord) -> Option<usize> {
    if hunk.added_lines.is_empty() {
        return locate_empty_inverse_hunk(current_lines, hunk);
    }

    let matches = find_line_window_matches(current_lines, &hunk.added_lines);
    if matches.len() == 1 {
        return matches.first().copied();
    }

    let context_matches = matches
        .into_iter()
        .filter(|start| hunk_context_matches(current_lines, *start, hunk.added_lines.len(), hunk))
        .collect::<Vec<_>>();
    if context_matches.len() == 1 {
        return context_matches.first().copied();
    }

    None
}

fn locate_empty_inverse_hunk(
    current_lines: &[String],
    hunk: &CodePatchHunkRecord,
) -> Option<usize> {
    if hunk.context_before.is_empty() && hunk.context_after.is_empty() {
        return current_lines.is_empty().then_some(0);
    }

    let expected_index = start_line_to_index(hunk.result_start_line);
    if boundary_context_matches(current_lines, expected_index, hunk) {
        return Some(expected_index);
    }

    let matches = (0..=current_lines.len())
        .filter(|position| boundary_context_matches(current_lines, *position, hunk))
        .collect::<Vec<_>>();
    (matches.len() == 1).then(|| matches[0])
}

fn find_line_window_matches(current_lines: &[String], needle: &[String]) -> Vec<usize> {
    if needle.is_empty() || needle.len() > current_lines.len() {
        return Vec::new();
    }

    (0..=current_lines.len() - needle.len())
        .filter(|start| line_window_matches(current_lines, *start, needle))
        .collect()
}

fn line_window_matches(current_lines: &[String], start: usize, needle: &[String]) -> bool {
    start
        .checked_add(needle.len())
        .and_then(|end| current_lines.get(start..end))
        .is_some_and(|window| window == needle)
}

fn hunk_context_matches(
    current_lines: &[String],
    start: usize,
    line_count: usize,
    hunk: &CodePatchHunkRecord,
) -> bool {
    let Some(end) = start.checked_add(line_count) else {
        return false;
    };
    context_before_matches(current_lines, start, &hunk.context_before)
        && context_after_matches(current_lines, end, &hunk.context_after)
}

fn boundary_context_matches(
    current_lines: &[String],
    position: usize,
    hunk: &CodePatchHunkRecord,
) -> bool {
    if position > current_lines.len() {
        return false;
    }
    context_before_matches(current_lines, position, &hunk.context_before)
        && context_after_matches(current_lines, position, &hunk.context_after)
}

fn context_before_matches(current_lines: &[String], start: usize, context: &[String]) -> bool {
    if context.is_empty() {
        return true;
    }
    start >= context.len() && current_lines[start - context.len()..start] == *context
}

fn context_after_matches(current_lines: &[String], end: usize, context: &[String]) -> bool {
    if context.is_empty() {
        return true;
    }
    end.checked_add(context.len())
        .and_then(|context_end| current_lines.get(end..context_end))
        .is_some_and(|window| window == context)
}

fn inverse_hunk_would_join_unrelated_lines(
    start: usize,
    removed_line_count: usize,
    inverse_added_lines: &[String],
    current_lines: &[String],
) -> bool {
    let Some(end) = start.checked_add(removed_line_count) else {
        return true;
    };
    end < current_lines.len()
        && inverse_added_lines
            .last()
            .is_some_and(|line| !line.ends_with('\n'))
}

fn split_text_lines_preserving_endings(text: &str) -> Vec<String> {
    text.split_inclusive('\n').map(ToOwned::to_owned).collect()
}

fn start_line_to_index(start_line: u32) -> usize {
    start_line.saturating_sub(1) as usize
}

fn index_to_start_line(index: usize) -> u32 {
    index.saturating_add(1).min(u32::MAX as usize) as u32
}

fn usize_to_u32_saturating(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
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

fn sha256_hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    let digest = Sha256::digest(bytes);
    let mut output = String::with_capacity(64);
    for byte in digest {
        write!(&mut output, "{byte:02x}").expect("writing to String should not fail");
    }
    output
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

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Mutex,
    };

    use tempfile::{tempdir, TempDir};

    use super::*;
    use crate::{
        commands::RuntimeAgentIdDto,
        db::{
            self,
            project_store::{
                append_agent_checkpoint, append_agent_file_change, append_agent_message,
                begin_exact_path_capture, complete_exact_path_capture, create_agent_session,
                insert_agent_run, update_agent_run_lineage, AgentMessageRole,
                AgentRunLineageUpdateRecord, AgentSessionCreateRecord, CodeChangeGroupInput,
                CodeChangeKind, CodeChangeRestoreState, CodeRollbackCaptureTarget,
                CompletedCodeChangeGroup, NewAgentCheckpointRecord, NewAgentFileChangeRecord,
                NewAgentMessageRecord, NewAgentRunRecord, DEFAULT_AGENT_SESSION_ID,
            },
            ProjectOrigin,
        },
        git::repository::CanonicalRepository,
        state::ImportFailpoints,
    };

    static PROJECT_DB_LOCK: Mutex<()> = Mutex::new(());

    struct TestProject {
        _tempdir: TempDir,
        repo_root: PathBuf,
        project_id: String,
        agent_session_id: String,
        run_id: String,
    }

    impl TestProject {
        fn new(label: &str) -> Self {
            let tempdir = tempdir().expect("tempdir");
            let app_data_dir = tempdir.path().join("app-data");
            let repo_root = tempdir.path().join(label);
            fs::create_dir_all(&repo_root).expect("repo root");
            db::configure_project_database_paths(&app_data_dir.join("global.db"));

            let project_id = format!("project_{label}");
            let repository = canonical_repository(&repo_root, &project_id);
            db::import_project_with_origin(
                &repository,
                ProjectOrigin::Brownfield,
                &ImportFailpoints::default(),
            )
            .expect("import project");
            let run_id = "run-1".to_string();
            insert_agent_run(
                &repo_root,
                &NewAgentRunRecord {
                    runtime_agent_id: RuntimeAgentIdDto::Engineer,
                    agent_definition_id: Some("engineer".into()),
                    agent_definition_version: Some(1),
                    project_id: project_id.clone(),
                    agent_session_id: DEFAULT_AGENT_SESSION_ID.into(),
                    run_id: run_id.clone(),
                    provider_id: "test-provider".into(),
                    model_id: "test-model".into(),
                    prompt: "test prompt".into(),
                    system_prompt: "test system prompt".into(),
                    now: "2026-05-06T12:00:00Z".into(),
                },
            )
            .expect("insert run");

            Self {
                _tempdir: tempdir,
                repo_root,
                project_id,
                agent_session_id: DEFAULT_AGENT_SESSION_ID.into(),
                run_id,
            }
        }

        fn input_for_run(&self, label: &str, run_id: &str) -> CodeChangeGroupInput {
            self.input_for_session_run_kind(
                label,
                &self.agent_session_id,
                run_id,
                CodeChangeKind::FileTool,
            )
        }

        fn input_for_session_run_kind(
            &self,
            label: &str,
            agent_session_id: &str,
            run_id: &str,
            change_kind: CodeChangeKind,
        ) -> CodeChangeGroupInput {
            CodeChangeGroupInput {
                project_id: self.project_id.clone(),
                agent_session_id: agent_session_id.into(),
                run_id: run_id.into(),
                change_group_id: None,
                parent_change_group_id: None,
                tool_call_id: Some(format!("tool-{label}")),
                runtime_event_id: None,
                conversation_sequence: None,
                change_kind,
                summary_label: label.into(),
                restore_state: CodeChangeRestoreState::SnapshotAvailable,
            }
        }
    }

    fn canonical_repository(root_path: &Path, project_id: &str) -> CanonicalRepository {
        let root_path = fs::canonicalize(root_path).expect("canonical repo root");
        CanonicalRepository {
            project_id: project_id.into(),
            repository_id: format!("repo_{project_id}"),
            root_path: root_path.clone(),
            root_path_string: root_path.to_string_lossy().into_owned(),
            common_git_dir: root_path.join(".git"),
            display_name: project_id.into(),
            branch_name: Some("main".into()),
            head_sha: Some("0123456789abcdef".into()),
            branch: None,
            last_commit: None,
            status_entries: Vec::new(),
            has_staged_changes: false,
            has_unstaged_changes: false,
            has_untracked_changes: false,
            additions: 0,
            deletions: 0,
        }
    }

    fn capture_modify_for_run(
        project: &TestProject,
        run_id: &str,
        label: &str,
        path: &str,
        after: &str,
    ) -> CompletedCodeChangeGroup {
        let handle = begin_exact_path_capture(
            &project.repo_root,
            project.input_for_run(label, run_id),
            vec![CodeRollbackCaptureTarget::modify(path)],
        )
        .expect("begin modify capture");
        fs::write(project.repo_root.join(path), after).expect("write changed file");
        complete_exact_path_capture(&project.repo_root, handle).expect("complete modify capture")
    }

    fn capture_modify_for_session_run_kind(
        project: &TestProject,
        agent_session_id: &str,
        run_id: &str,
        change_kind: CodeChangeKind,
        label: &str,
        path: &str,
        after: &str,
    ) -> CompletedCodeChangeGroup {
        let handle = begin_exact_path_capture(
            &project.repo_root,
            project.input_for_session_run_kind(label, agent_session_id, run_id, change_kind),
            vec![CodeRollbackCaptureTarget::modify(path)],
        )
        .expect("begin modify capture");
        fs::write(project.repo_root.join(path), after).expect("write changed file");
        complete_exact_path_capture(&project.repo_root, handle).expect("complete modify capture")
    }

    fn insert_child_run(project: &TestProject, child_run_id: &str) {
        insert_agent_run_for_session(
            project,
            &project.agent_session_id,
            child_run_id,
            "2026-05-06T12:00:01Z",
        );
        update_agent_run_lineage(
            &project.repo_root,
            &AgentRunLineageUpdateRecord {
                project_id: project.project_id.clone(),
                run_id: child_run_id.into(),
                parent_run_id: project.run_id.clone(),
                parent_trace_id: xero_agent_core::runtime_trace_id_for_run(
                    &project.project_id,
                    &project.run_id,
                ),
                parent_subagent_id: "subagent-1".into(),
                subagent_role: "engineer".into(),
                updated_at: "2026-05-06T12:00:02Z".into(),
            },
        )
        .expect("attach child lineage");
    }

    fn insert_agent_run_for_session(
        project: &TestProject,
        agent_session_id: &str,
        run_id: &str,
        now: &str,
    ) {
        insert_agent_run(
            &project.repo_root,
            &NewAgentRunRecord {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                agent_definition_id: Some("engineer".into()),
                agent_definition_version: Some(1),
                project_id: project.project_id.clone(),
                agent_session_id: agent_session_id.into(),
                run_id: run_id.into(),
                provider_id: "test-provider".into(),
                model_id: "test-model".into(),
                prompt: "test prompt".into(),
                system_prompt: "test system prompt".into(),
                now: now.into(),
            },
        )
        .expect("insert agent run");
    }

    #[test]
    fn resolve_code_session_boundary_maps_top_level_run_boundary_to_latest_commit() {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_boundary_top_level_run");
        fs::write(project.repo_root.join("tracked.txt"), "before\n").expect("baseline");
        let group = capture_modify_for_run(
            &project,
            &project.run_id,
            "edit tracked",
            "tracked.txt",
            "after\n",
        );

        let resolved = resolve_code_session_boundary(
            &project.repo_root,
            &ResolveCodeSessionBoundaryRequest {
                project_id: project.project_id.clone(),
                agent_session_id: project.agent_session_id.clone(),
                target_kind: CodeSessionBoundaryTargetKind::RunBoundary,
                target_id: format!("run:{}", project.run_id),
                boundary_id: format!("run:{}", project.run_id),
                run_id: Some(project.run_id.clone()),
                change_group_id: None,
            },
        )
        .expect("resolve run boundary");

        assert_eq!(resolved.source_kind, CodeSessionBoundarySourceKind::Run);
        assert_eq!(resolved.commit.change_group_id, group.change_group_id);
        assert_eq!(resolved.lineage.boundary_run_id, project.run_id);
        assert_eq!(resolved.lineage.parent_run_id, None);
        assert_eq!(resolved.lineage.included_run_ids, vec!["run-1".to_string()]);
    }

    #[test]
    fn resolve_code_session_boundary_maps_child_run_boundary_to_child_lineage() {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_boundary_child_run");
        let child_run_id = "child-run-1";
        insert_child_run(&project, child_run_id);
        fs::write(project.repo_root.join("child.txt"), "child before\n").expect("baseline");
        let group = capture_modify_for_run(
            &project,
            child_run_id,
            "child edit",
            "child.txt",
            "child after\n",
        );

        let resolved = resolve_code_session_boundary(
            &project.repo_root,
            &ResolveCodeSessionBoundaryRequest {
                project_id: project.project_id.clone(),
                agent_session_id: project.agent_session_id.clone(),
                target_kind: CodeSessionBoundaryTargetKind::RunBoundary,
                target_id: format!("run:{child_run_id}"),
                boundary_id: format!("run:{child_run_id}"),
                run_id: Some(child_run_id.into()),
                change_group_id: None,
            },
        )
        .expect("resolve child run boundary");

        assert_eq!(resolved.commit.run_id, child_run_id);
        assert_eq!(resolved.commit.change_group_id, group.change_group_id);
        assert_eq!(resolved.lineage.root_run_id, project.run_id);
        assert_eq!(resolved.lineage.parent_run_id.as_deref(), Some("run-1"));
        assert_eq!(
            resolved.lineage.included_run_ids,
            vec![child_run_id.to_string()]
        );
    }

    #[test]
    fn resolve_code_session_boundary_maps_change_group_file_change_and_checkpoint_boundaries() {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_boundary_sources");
        fs::write(project.repo_root.join("tracked.txt"), "before\n").expect("baseline");
        let group = capture_modify_for_run(
            &project,
            &project.run_id,
            "edit tracked",
            "tracked.txt",
            "after\n",
        );

        let change_group_boundary = resolve_code_session_boundary(
            &project.repo_root,
            &ResolveCodeSessionBoundaryRequest {
                project_id: project.project_id.clone(),
                agent_session_id: project.agent_session_id.clone(),
                target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                target_id: format!("change_group:{}", group.change_group_id),
                boundary_id: format!("change_group:{}", group.change_group_id),
                run_id: None,
                change_group_id: Some(group.change_group_id.clone()),
            },
        )
        .expect("resolve change group");
        assert_eq!(
            change_group_boundary.source_kind,
            CodeSessionBoundarySourceKind::ChangeGroup
        );

        let file_change = append_agent_file_change(
            &project.repo_root,
            &NewAgentFileChangeRecord {
                project_id: project.project_id.clone(),
                run_id: project.run_id.clone(),
                change_group_id: Some(group.change_group_id.clone()),
                path: "tracked.txt".into(),
                operation: "edit".into(),
                old_hash: None,
                new_hash: None,
                created_at: "2026-05-06T12:01:00Z".into(),
            },
        )
        .expect("append file change");
        let file_change_boundary = resolve_code_session_boundary(
            &project.repo_root,
            &ResolveCodeSessionBoundaryRequest {
                project_id: project.project_id.clone(),
                agent_session_id: project.agent_session_id.clone(),
                target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                target_id: format!("file_change:{}", file_change.id),
                boundary_id: format!("file_change:{}", file_change.id),
                run_id: None,
                change_group_id: None,
            },
        )
        .expect("resolve file change");
        assert_eq!(
            file_change_boundary.commit.commit_id,
            change_group_boundary.commit.commit_id
        );

        let checkpoint = append_agent_checkpoint(
            &project.repo_root,
            &NewAgentCheckpointRecord {
                project_id: project.project_id.clone(),
                run_id: project.run_id.clone(),
                checkpoint_kind: "tool".into(),
                summary: "after code".into(),
                payload_json: None,
                created_at: "2999-01-01T00:00:00Z".into(),
            },
        )
        .expect("append checkpoint");
        let checkpoint_boundary = resolve_code_session_boundary(
            &project.repo_root,
            &ResolveCodeSessionBoundaryRequest {
                project_id: project.project_id.clone(),
                agent_session_id: project.agent_session_id.clone(),
                target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                target_id: format!("checkpoint:{}", checkpoint.id),
                boundary_id: format!("checkpoint:{}", checkpoint.id),
                run_id: None,
                change_group_id: None,
            },
        )
        .expect("resolve checkpoint");
        assert_eq!(
            checkpoint_boundary.source_kind,
            CodeSessionBoundarySourceKind::Checkpoint
        );
        assert_eq!(
            checkpoint_boundary.commit.change_group_id,
            group.change_group_id
        );
    }

    #[test]
    fn build_code_session_lineage_undo_plan_collects_later_session_changes_newest_first() {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_lineage_plan_session");
        fs::write(project.repo_root.join("boundary.txt"), "boundary before\n")
            .expect("boundary baseline");
        let boundary_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "boundary edit",
            "boundary.txt",
            "boundary after\n",
        );

        fs::write(project.repo_root.join("later.txt"), "later before\n").expect("later baseline");
        let later_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "later edit",
            "later.txt",
            "later after\n",
        );

        let child_run_id = "child-run-plan";
        insert_child_run(&project, child_run_id);
        fs::write(project.repo_root.join("child.txt"), "child before\n").expect("child baseline");
        let child_group = capture_modify_for_run(
            &project,
            child_run_id,
            "child later edit",
            "child.txt",
            "child after\n",
        );

        let plan = build_code_session_lineage_undo_plan(
            &project.repo_root,
            &BuildCodeSessionLineageUndoPlanRequest {
                boundary: ResolveCodeSessionBoundaryRequest {
                    project_id: project.project_id.clone(),
                    agent_session_id: project.agent_session_id.clone(),
                    target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                    target_id: format!("change_group:{}", boundary_group.change_group_id),
                    boundary_id: format!("change_group:{}", boundary_group.change_group_id),
                    run_id: None,
                    change_group_id: Some(boundary_group.change_group_id.clone()),
                },
                explicitly_selected_change_group_ids: Vec::new(),
            },
        )
        .expect("build session lineage plan");

        let target_ids = plan
            .target_change_groups
            .iter()
            .map(|group| group.change_group_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            target_ids,
            vec![
                child_group.change_group_id.as_str(),
                later_group.change_group_id.as_str()
            ]
        );
        assert!(plan
            .target_change_groups
            .iter()
            .all(|group| !group.explicitly_selected));
        assert_eq!(plan.affected_paths, vec!["child.txt", "later.txt"]);
    }

    #[test]
    fn build_code_session_lineage_undo_plan_rejects_over_budget_commit_scans() {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_lineage_plan_budget");
        fs::write(project.repo_root.join("boundary.txt"), "boundary before\n")
            .expect("boundary baseline");
        let boundary_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "boundary edit",
            "boundary.txt",
            "boundary after\n",
        );

        for index in 0..3 {
            let path = format!("later-{index}.txt");
            fs::write(project.repo_root.join(&path), "before\n").expect("later baseline");
            capture_modify_for_run(
                &project,
                &project.run_id,
                &format!("later edit {index}"),
                &path,
                "after\n",
            );
        }

        let error = build_code_session_lineage_undo_plan_with_budget(
            &project.repo_root,
            &BuildCodeSessionLineageUndoPlanRequest {
                boundary: ResolveCodeSessionBoundaryRequest {
                    project_id: project.project_id.clone(),
                    agent_session_id: project.agent_session_id.clone(),
                    target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                    target_id: format!("change_group:{}", boundary_group.change_group_id),
                    boundary_id: format!("change_group:{}", boundary_group.change_group_id),
                    run_id: None,
                    change_group_id: Some(boundary_group.change_group_id.clone()),
                },
                explicitly_selected_change_group_ids: Vec::new(),
            },
            CodeSessionLineageUndoPlanBudget {
                max_commits_after_boundary: 2,
                max_target_change_groups: 10,
                max_excluded_change_groups: 10,
                max_patch_files: 10,
                max_affected_paths: 10,
            },
        )
        .expect_err("budget exceeded");

        assert_eq!(error.code, "code_session_lineage_undo_plan_budget_exceeded");
        assert!(error.message.contains("commitsAfterBoundary"));
    }

    #[test]
    fn build_code_session_lineage_undo_plan_excludes_sibling_and_recovered_changes_by_default() {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_lineage_plan_exclusions");
        fs::write(project.repo_root.join("boundary.txt"), "boundary before\n")
            .expect("boundary baseline");
        let boundary_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "boundary edit",
            "boundary.txt",
            "boundary after\n",
        );

        fs::write(project.repo_root.join("owned.txt"), "owned before\n").expect("owned baseline");
        let owned_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "owned later edit",
            "owned.txt",
            "owned after\n",
        );

        let sibling_session = create_agent_session(
            &project.repo_root,
            &AgentSessionCreateRecord {
                project_id: project.project_id.clone(),
                title: "Sibling".into(),
                summary: String::new(),
                selected: false,
            },
        )
        .expect("create sibling session");
        let sibling_session_id = sibling_session.agent_session_id;
        let sibling_run_id = "sibling-run";
        insert_agent_run_for_session(
            &project,
            &sibling_session_id,
            sibling_run_id,
            "2026-05-06T12:00:03Z",
        );
        fs::write(project.repo_root.join("sibling.txt"), "sibling before\n")
            .expect("sibling baseline");
        let sibling_group = capture_modify_for_session_run_kind(
            &project,
            &sibling_session_id,
            sibling_run_id,
            CodeChangeKind::FileTool,
            "sibling edit",
            "sibling.txt",
            "sibling after\n",
        );

        fs::write(
            project.repo_root.join("recovered.txt"),
            "recovered before\n",
        )
        .expect("recovered baseline");
        let recovered_group = capture_modify_for_session_run_kind(
            &project,
            &project.agent_session_id,
            &project.run_id,
            CodeChangeKind::RecoveredMutation,
            "recovered probe edit",
            "recovered.txt",
            "recovered after\n",
        );

        fs::write(
            project.repo_root.join("owned-newest.txt"),
            "owned newest before\n",
        )
        .expect("owned newest baseline");
        let owned_newest_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "owned newest edit",
            "owned-newest.txt",
            "owned newest after\n",
        );

        let boundary_request = ResolveCodeSessionBoundaryRequest {
            project_id: project.project_id.clone(),
            agent_session_id: project.agent_session_id.clone(),
            target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
            target_id: format!("change_group:{}", boundary_group.change_group_id),
            boundary_id: format!("change_group:{}", boundary_group.change_group_id),
            run_id: None,
            change_group_id: Some(boundary_group.change_group_id.clone()),
        };
        let default_plan = build_code_session_lineage_undo_plan(
            &project.repo_root,
            &BuildCodeSessionLineageUndoPlanRequest {
                boundary: boundary_request.clone(),
                explicitly_selected_change_group_ids: Vec::new(),
            },
        )
        .expect("build default plan");

        let default_target_ids = default_plan
            .target_change_groups
            .iter()
            .map(|group| group.change_group_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            default_target_ids,
            vec![
                owned_newest_group.change_group_id.as_str(),
                owned_group.change_group_id.as_str()
            ]
        );
        assert!(default_plan.excluded_change_groups.iter().any(|group| {
            group.change_group_id == sibling_group.change_group_id
                && group.reason == CodeSessionLineageUndoPlanExclusionReason::SiblingSession
        }));
        assert!(default_plan.excluded_change_groups.iter().any(|group| {
            group.change_group_id == recovered_group.change_group_id
                && group.reason == CodeSessionLineageUndoPlanExclusionReason::UserOrRecoveredChange
        }));

        let explicit_plan = build_code_session_lineage_undo_plan(
            &project.repo_root,
            &BuildCodeSessionLineageUndoPlanRequest {
                boundary: boundary_request,
                explicitly_selected_change_group_ids: vec![
                    sibling_group.change_group_id.clone(),
                    recovered_group.change_group_id.clone(),
                ],
            },
        )
        .expect("build explicit plan");
        let explicit_target_ids = explicit_plan
            .target_change_groups
            .iter()
            .map(|group| group.change_group_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            explicit_target_ids,
            vec![
                owned_newest_group.change_group_id.as_str(),
                recovered_group.change_group_id.as_str(),
                sibling_group.change_group_id.as_str(),
                owned_group.change_group_id.as_str()
            ]
        );
        assert!(explicit_plan
            .target_change_groups
            .iter()
            .filter(|group| group.explicitly_selected)
            .all(|group| {
                group.change_group_id == sibling_group.change_group_id
                    || group.change_group_id == recovered_group.change_group_id
            }));
    }

    #[test]
    fn build_code_session_lineage_undo_plan_respects_run_boundary_scope() {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_lineage_plan_run_scope");
        let child_run_id = "child-run-scope";
        insert_child_run(&project, child_run_id);

        fs::write(
            project.repo_root.join("child-boundary.txt"),
            "child boundary before\n",
        )
        .expect("child boundary baseline");
        let child_boundary_group = capture_modify_for_run(
            &project,
            child_run_id,
            "child boundary edit",
            "child-boundary.txt",
            "child boundary after\n",
        );

        fs::write(
            project.repo_root.join("parent-later.txt"),
            "parent before\n",
        )
        .expect("parent baseline");
        let parent_later_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "parent later edit",
            "parent-later.txt",
            "parent after\n",
        );

        fs::write(project.repo_root.join("child-later.txt"), "child before\n")
            .expect("child baseline");
        let child_later_group = capture_modify_for_run(
            &project,
            child_run_id,
            "child later edit",
            "child-later.txt",
            "child after\n",
        );

        let plan = build_code_session_lineage_undo_plan(
            &project.repo_root,
            &BuildCodeSessionLineageUndoPlanRequest {
                boundary: ResolveCodeSessionBoundaryRequest {
                    project_id: project.project_id.clone(),
                    agent_session_id: project.agent_session_id.clone(),
                    target_kind: CodeSessionBoundaryTargetKind::RunBoundary,
                    target_id: format!("run:{child_run_id}"),
                    boundary_id: format!("change_group:{}", child_boundary_group.change_group_id),
                    run_id: Some(child_run_id.into()),
                    change_group_id: Some(child_boundary_group.change_group_id.clone()),
                },
                explicitly_selected_change_group_ids: Vec::new(),
            },
        )
        .expect("build run-boundary plan");

        let target_ids = plan
            .target_change_groups
            .iter()
            .map(|group| group.change_group_id.as_str())
            .collect::<Vec<_>>();
        assert_eq!(target_ids, vec![child_later_group.change_group_id.as_str()]);
        assert!(plan.excluded_change_groups.iter().any(|group| {
            group.change_group_id == parent_later_group.change_group_id
                && group.reason == CodeSessionLineageUndoPlanExclusionReason::OutsideRunLineage
        }));
    }

    #[test]
    fn build_code_session_lineage_undo_plan_preserves_dirty_current_overlay_for_clean_text_rollback(
    ) {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_lineage_plan_dirty_overlay_clean");
        fs::write(project.repo_root.join("boundary.txt"), "boundary before\n")
            .expect("boundary baseline");
        let boundary_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "boundary edit",
            "boundary.txt",
            "boundary after\n",
        );

        fs::write(project.repo_root.join("story.txt"), "agent old\nstable\n")
            .expect("story baseline");
        let first_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "story first edit",
            "story.txt",
            "agent middle\nstable\n",
        );
        let second_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "story second edit",
            "story.txt",
            "agent new\nstable\n",
        );
        fs::write(
            project.repo_root.join("story.txt"),
            "agent new\nstable\nuser note\n",
        )
        .expect("dirty user overlay");

        let plan = build_code_session_lineage_undo_plan(
            &project.repo_root,
            &BuildCodeSessionLineageUndoPlanRequest {
                boundary: ResolveCodeSessionBoundaryRequest {
                    project_id: project.project_id.clone(),
                    agent_session_id: project.agent_session_id.clone(),
                    target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                    target_id: format!("change_group:{}", boundary_group.change_group_id),
                    boundary_id: format!("change_group:{}", boundary_group.change_group_id),
                    run_id: None,
                    change_group_id: Some(boundary_group.change_group_id.clone()),
                },
                explicitly_selected_change_group_ids: Vec::new(),
            },
        )
        .expect("build dirty overlay plan");

        assert_eq!(plan.status, CodeSessionLineageUndoPlanStatus::Clean);
        assert_eq!(
            plan.conflicts,
            Vec::<CodeSessionLineageUndoPlanConflict>::new()
        );
        assert_eq!(
            plan.target_change_groups
                .iter()
                .map(|group| group.change_group_id.as_str())
                .collect::<Vec<_>>(),
            vec![
                second_group.change_group_id.as_str(),
                first_group.change_group_id.as_str()
            ]
        );
        assert!(plan
            .preserved_dirty_overlays
            .iter()
            .all(|overlay| overlay.path == "story.txt"));
        assert!(!plan.preserved_dirty_overlays.is_empty());

        let final_story_plan = plan
            .planned_files
            .iter()
            .find(|file| file.change_group_id == first_group.change_group_id)
            .expect("final story inverse plan");
        assert!(final_story_plan.preserved_dirty_overlay);
        assert_eq!(
            final_story_plan.planned_content.as_deref(),
            Some("agent old\nstable\nuser note\n")
        );
        assert_eq!(
            fs::read_to_string(project.repo_root.join("story.txt")).expect("read story"),
            "agent new\nstable\nuser note\n"
        );
    }

    #[test]
    fn build_code_session_lineage_undo_plan_conflicts_when_dirty_overlay_overlaps_target_hunk() {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_lineage_plan_dirty_overlay_conflict");
        fs::write(project.repo_root.join("boundary.txt"), "boundary before\n")
            .expect("boundary baseline");
        let boundary_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "boundary edit",
            "boundary.txt",
            "boundary after\n",
        );

        fs::write(project.repo_root.join("story.txt"), "agent old\nstable\n")
            .expect("story baseline");
        let story_group = capture_modify_for_run(
            &project,
            &project.run_id,
            "story edit",
            "story.txt",
            "agent new\nstable\n",
        );
        fs::write(
            project.repo_root.join("story.txt"),
            "human overlap\nstable\n",
        )
        .expect("dirty overlapping edit");

        let plan = build_code_session_lineage_undo_plan(
            &project.repo_root,
            &BuildCodeSessionLineageUndoPlanRequest {
                boundary: ResolveCodeSessionBoundaryRequest {
                    project_id: project.project_id.clone(),
                    agent_session_id: project.agent_session_id.clone(),
                    target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                    target_id: format!("change_group:{}", boundary_group.change_group_id),
                    boundary_id: format!("change_group:{}", boundary_group.change_group_id),
                    run_id: None,
                    change_group_id: Some(boundary_group.change_group_id.clone()),
                },
                explicitly_selected_change_group_ids: Vec::new(),
            },
        )
        .expect("build conflicted dirty overlay plan");

        assert_eq!(plan.status, CodeSessionLineageUndoPlanStatus::Conflicted);
        assert!(plan.conflicts.iter().any(|conflict| {
            conflict.change_group_id == story_group.change_group_id
                && conflict.path == "story.txt"
                && conflict.kind == CodeSessionLineageUndoPlanConflictKind::TextOverlap
        }));
        assert_eq!(
            fs::read_to_string(project.repo_root.join("story.txt")).expect("read story"),
            "human overlap\nstable\n"
        );
    }

    #[test]
    fn resolve_code_session_boundary_returns_deterministic_errors_for_missing_and_non_code_boundaries(
    ) {
        let _guard = PROJECT_DB_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let project = TestProject::new("history_boundary_errors");

        let missing = resolve_code_session_boundary(
            &project.repo_root,
            &ResolveCodeSessionBoundaryRequest {
                project_id: project.project_id.clone(),
                agent_session_id: project.agent_session_id.clone(),
                target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                target_id: "checkpoint:404".into(),
                boundary_id: "checkpoint:404".into(),
                run_id: None,
                change_group_id: None,
            },
        )
        .expect_err("missing checkpoint should fail");
        assert_eq!(missing.code, "code_session_boundary_not_found");

        let message = append_agent_message(
            &project.repo_root,
            &NewAgentMessageRecord {
                project_id: project.project_id.clone(),
                run_id: project.run_id.clone(),
                role: AgentMessageRole::User,
                content: "No code has happened yet.".into(),
                provider_metadata_json: None,
                created_at: "2026-05-06T12:02:00Z".into(),
                attachments: Vec::new(),
            },
        )
        .expect("append message");
        let non_code = resolve_code_session_boundary(
            &project.repo_root,
            &ResolveCodeSessionBoundaryRequest {
                project_id: project.project_id.clone(),
                agent_session_id: project.agent_session_id.clone(),
                target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                target_id: format!("message:{}", message.id),
                boundary_id: format!("message:{}", message.id),
                run_id: None,
                change_group_id: None,
            },
        )
        .expect_err("non-code message should fail");
        assert_eq!(non_code.code, "code_session_boundary_no_code_commit");
    }
}
