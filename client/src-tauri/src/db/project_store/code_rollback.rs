use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Component, Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

use ignore::{DirEntry, WalkBuilder};
use rand::RngCore;
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value as JsonValue};
use sha2::{Digest, Sha256};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::{database_path_for_repo, project_app_data_dir_for_repo},
};

use super::{
    code_history::{
        advance_code_workspace_epoch, build_code_session_lineage_undo_plan,
        code_change_group_history_metadata_from_commit, ensure_code_workspace_head,
        persist_code_patchset_commit, plan_file_operation_inverse_patch,
        plan_text_file_inverse_patch, read_code_change_group_history_metadata,
        read_code_patchset_commit, AdvanceCodeWorkspaceEpochRequest,
        BuildCodeSessionLineageUndoPlanRequest, CodeChangeGroupHistoryMetadataRecord,
        CodeExactFileState, CodeFileOperationCurrentState, CodeFileOperationInverseAction,
        CodeFileOperationInverseActionKind, CodeFileOperationInverseConflict,
        CodeFileOperationInverseConflictKind, CodeFileOperationInversePatchPlanStatus,
        CodeHistoryCommitKind, CodePatchFileInput, CodePatchFileKind, CodePatchFileOperation,
        CodePatchFileRecord, CodePatchHunkInput, CodePatchMergePolicy, CodePatchsetCommitInput,
        CodePatchsetCommitRecord, CodeSessionBoundaryTargetKind, CodeSessionLineageUndoPlan,
        CodeSessionLineageUndoPlanConflict, CodeSessionLineageUndoPlanConflictKind,
        CodeSessionLineageUndoPlanFile, CodeSessionLineageUndoPlanStatus,
        CodeTextInversePatchConflict, CodeTextInversePatchConflictKind,
        CodeTextInversePatchPlanStatus, CodeWorkspaceHeadRecord, ResolveCodeSessionBoundaryRequest,
    },
    coordination_paths_overlap, open_runtime_database, read_project_row, AgentMailboxItemType,
    AgentMailboxPriority, InvalidateAgentFileReservationsRequest,
};

const SNAPSHOT_SCHEMA: &str = "xero.code_snapshot.v1";
const CODE_ROLLBACK_DIR: &str = "code-rollback";
const BLOB_DIR: &str = "blobs";
const DIAGNOSTICS_DIR: &str = "diagnostics";
const MAINTENANCE_REPORT_FILE: &str = "latest-maintenance.json";
const DEFAULT_UNREFERENCED_BLOB_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;
const DEFAULT_HISTORY_COORDINATION_EVENT_LEASE_SECONDS: i64 = 3_600;
const DEFAULT_SNAPSHOT_SCAN_MAX_WALK_ENTRIES: usize = 50_000;
const DEFAULT_SNAPSHOT_SCAN_MAX_NEW_BLOB_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_BROAD_CAPTURE_NON_EXPLICIT_DIFF_FILE_LIMIT: usize = 2_000;
const DEFAULT_TEXT_HUNK_PAYLOAD_BYTES: usize = 256 * 1024;

static CODE_RESTORE_LOCKS: LazyLock<Mutex<BTreeMap<String, Arc<Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

const DEFAULT_SKIPPED_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".next",
    ".turbo",
    ".svelte-kit",
    "coverage",
    ".xero",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodeSnapshotFileKind {
    File,
    Directory,
    Symlink,
}

impl CodeSnapshotFileKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::Directory => "directory",
            Self::Symlink => "symlink",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeSnapshotBoundaryKind {
    Before,
    After,
    Baseline,
    PreRollback,
    PostRollback,
    Manual,
}

impl CodeSnapshotBoundaryKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Before => "before",
            Self::After => "after",
            Self::Baseline => "baseline",
            Self::PreRollback => "pre_rollback",
            Self::PostRollback => "post_rollback",
            Self::Manual => "manual",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeChangeKind {
    FileTool,
    Command,
    Mcp,
    Rollback,
    RecoveredMutation,
    ImportedBaseline,
}

impl CodeChangeKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::FileTool => "file_tool",
            Self::Command => "command",
            Self::Mcp => "mcp",
            Self::Rollback => "rollback",
            Self::RecoveredMutation => "recovered_mutation",
            Self::ImportedBaseline => "imported_baseline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeChangeRestoreState {
    SnapshotAvailable,
    SnapshotMissing,
    ExternalEffectsUntracked,
}

impl CodeChangeRestoreState {
    fn as_str(self) -> &'static str {
        match self {
            Self::SnapshotAvailable => "snapshot_available",
            Self::SnapshotMissing => "snapshot_missing",
            Self::ExternalEffectsUntracked => "external_effects_untracked",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeFileOperation {
    Create,
    Modify,
    Delete,
    Rename,
    ModeChange,
    SymlinkChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeHistoryOperationMode {
    SelectiveUndo,
    SessionRollback,
}

impl CodeHistoryOperationMode {
    fn as_str(self) -> &'static str {
        match self {
            Self::SelectiveUndo => "selective_undo",
            Self::SessionRollback => "session_rollback",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeHistoryOperationStatus {
    Pending,
    Planning,
    Conflicted,
    Applying,
    Completed,
    Failed,
    RepairNeeded,
}

impl CodeHistoryOperationStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Planning => "planning",
            Self::Conflicted => "conflicted",
            Self::Applying => "applying",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::RepairNeeded => "repair_needed",
        }
    }

    fn from_sql(value: &str) -> CommandResult<Self> {
        match value {
            "pending" => Ok(Self::Pending),
            "planning" => Ok(Self::Planning),
            "conflicted" => Ok(Self::Conflicted),
            "applying" => Ok(Self::Applying),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "repair_needed" => Ok(Self::RepairNeeded),
            _ => Err(CommandError::system_fault(
                "code_history_operation_status_invalid",
                format!("Code history operation status `{value}` is not recognized."),
            )),
        }
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Conflicted | Self::Completed | Self::Failed | Self::RepairNeeded
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CodeHistoryOperationTargetKind {
    ChangeGroup,
    FileChange,
    Hunks,
    SessionBoundary,
    RunBoundary,
}

impl CodeHistoryOperationTargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::ChangeGroup => "change_group",
            Self::FileChange => "file_change",
            Self::Hunks => "hunks",
            Self::SessionBoundary => "session_boundary",
            Self::RunBoundary => "run_boundary",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedCodeRollback {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub operation_id: String,
    pub target_change_group_id: String,
    pub target_snapshot_id: String,
    pub pre_rollback_snapshot_id: String,
    pub result_change_group_id: String,
    pub restored_paths: Vec<String>,
    pub removed_paths: Vec<String>,
    pub affected_files: Vec<CompletedCodeChangeFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyCodeFileUndoRequest {
    pub project_id: String,
    pub operation_id: Option<String>,
    pub target_change_group_id: String,
    pub target_patch_file_id: Option<String>,
    pub target_file_path: Option<String>,
    pub target_hunk_ids: Vec<String>,
    pub expected_workspace_epoch: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyCodeChangeGroupUndoRequest {
    pub project_id: String,
    pub operation_id: Option<String>,
    pub target_change_group_id: String,
    pub expected_workspace_epoch: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyCodeSessionRollbackRequest {
    pub boundary: ResolveCodeSessionBoundaryRequest,
    pub operation_id: Option<String>,
    pub explicitly_selected_change_group_ids: Vec<String>,
    pub expected_workspace_epoch: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeFileUndoApplyStatus {
    Completed,
    Conflicted,
}

impl CodeFileUndoApplyStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Conflicted => "conflicted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeFileUndoConflictKind {
    TextOverlap,
    FileMissing,
    FileExists,
    ContentMismatch,
    MetadataMismatch,
    UnsupportedOperation,
    StaleWorkspace,
}

impl CodeFileUndoConflictKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::TextOverlap => "text_overlap",
            Self::FileMissing => "file_missing",
            Self::FileExists => "file_exists",
            Self::ContentMismatch => "content_mismatch",
            Self::MetadataMismatch => "metadata_mismatch",
            Self::UnsupportedOperation => "unsupported_operation",
            Self::StaleWorkspace => "stale_workspace",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeFileUndoConflict {
    pub path: String,
    pub kind: CodeFileUndoConflictKind,
    pub message: String,
    pub base_hash: Option<String>,
    pub selected_hash: Option<String>,
    pub current_hash: Option<String>,
    pub hunk_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedCodeFileUndo {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub operation_id: String,
    pub status: CodeFileUndoApplyStatus,
    pub target_change_group_id: String,
    pub target_patch_file_id: String,
    pub target_file_path: String,
    pub selected_hunk_ids: Vec<String>,
    pub result_change_group_id: Option<String>,
    pub result_commit_id: Option<String>,
    pub affected_paths: Vec<String>,
    pub conflicts: Vec<CodeFileUndoConflict>,
    pub workspace_head: Option<CodeWorkspaceHeadRecord>,
    pub patch_availability: Option<CodeChangeGroupHistoryMetadataRecord>,
    pub affected_files: Vec<CompletedCodeChangeFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedCodeChangeGroupUndo {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub operation_id: String,
    pub status: CodeFileUndoApplyStatus,
    pub target_change_group_id: String,
    pub result_change_group_id: Option<String>,
    pub result_commit_id: Option<String>,
    pub affected_paths: Vec<String>,
    pub conflicts: Vec<CodeFileUndoConflict>,
    pub workspace_head: Option<CodeWorkspaceHeadRecord>,
    pub patch_availability: Option<CodeChangeGroupHistoryMetadataRecord>,
    pub affected_files: Vec<CompletedCodeChangeFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedCodeSessionRollback {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub operation_id: String,
    pub status: CodeFileUndoApplyStatus,
    pub target_id: String,
    pub boundary_id: String,
    pub boundary_change_group_id: Option<String>,
    pub target_change_group_ids: Vec<String>,
    pub result_change_group_id: Option<String>,
    pub result_commit_id: Option<String>,
    pub affected_paths: Vec<String>,
    pub conflicts: Vec<CodeFileUndoConflict>,
    pub workspace_head: Option<CodeWorkspaceHeadRecord>,
    pub patch_availability: Option<CodeChangeGroupHistoryMetadataRecord>,
    pub affected_files: Vec<CompletedCodeChangeFile>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRollbackOperationRecord {
    pub project_id: String,
    pub operation_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub target_change_group_id: String,
    pub target_snapshot_id: String,
    pub pre_rollback_snapshot_id: Option<String>,
    pub result_change_group_id: Option<String>,
    pub status: String,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
    pub affected_files: Vec<CompletedCodeChangeFile>,
    pub target_summary_label: Option<String>,
    pub result_summary_label: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeHistoryOperationConflictRecord {
    pub path: String,
    pub kind: String,
    pub message: String,
    pub base_hash: Option<String>,
    pub selected_hash: Option<String>,
    pub current_hash: Option<String>,
    pub hunk_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeHistoryOperationRecord {
    pub project_id: String,
    pub operation_id: String,
    pub mode: String,
    pub status: String,
    pub target_kind: String,
    pub target_id: String,
    pub target_change_group_id: Option<String>,
    pub target_file_path: Option<String>,
    pub target_hunk_ids: Vec<String>,
    pub agent_session_id: String,
    pub run_id: String,
    pub expected_workspace_epoch: Option<u64>,
    pub affected_paths: Vec<String>,
    pub conflicts: Vec<CodeHistoryOperationConflictRecord>,
    pub result_change_group_id: Option<String>,
    pub result_commit_id: Option<String>,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
    pub repair_code: Option<String>,
    pub repair_message: Option<String>,
    pub target_summary_label: Option<String>,
    pub result_summary_label: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub completed_at: Option<String>,
}

impl CodeFileOperation {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Create => "create",
            Self::Modify => "modify",
            Self::Delete => "delete",
            Self::Rename => "rename",
            Self::ModeChange => "mode_change",
            Self::SymlinkChange => "symlink_change",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSnapshotFileEntry {
    pub path: String,
    pub kind: CodeSnapshotFileKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at_millis: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub symlink_target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeSnapshotManifest {
    pub schema: String,
    pub root_path: String,
    pub captured_at: String,
    pub entries: Vec<CodeSnapshotFileEntry>,
}

impl CodeSnapshotManifest {
    fn new(root_path: &Path, captured_at: String, entries: Vec<CodeSnapshotFileEntry>) -> Self {
        Self {
            schema: SNAPSHOT_SCHEMA.into(),
            root_path: root_path.to_string_lossy().into_owned(),
            captured_at,
            entries,
        }
    }

    fn entry_map(&self) -> BTreeMap<String, CodeSnapshotFileEntry> {
        self.entries
            .iter()
            .map(|entry| (entry.path.clone(), entry.clone()))
            .collect()
    }

    fn total_file_bytes(&self) -> u64 {
        self.entries
            .iter()
            .filter(|entry| entry.kind == CodeSnapshotFileKind::File)
            .filter_map(|entry| entry.size)
            .sum()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeSnapshotRecord {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub snapshot_id: String,
    pub change_group_id: Option<String>,
    pub boundary_kind: CodeSnapshotBoundaryKind,
    pub manifest: CodeSnapshotManifest,
    pub created_at: String,
    pub completed_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeChangeGroupInput {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub change_group_id: Option<String>,
    pub parent_change_group_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub runtime_event_id: Option<i64>,
    pub conversation_sequence: Option<i64>,
    pub change_kind: CodeChangeKind,
    pub summary_label: String,
    pub restore_state: CodeChangeRestoreState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRollbackCaptureTarget {
    pub path_before: Option<String>,
    pub path_after: Option<String>,
    pub operation: Option<CodeFileOperation>,
    pub explicitly_edited: bool,
}

impl CodeRollbackCaptureTarget {
    pub fn modify(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            path_before: Some(path.clone()),
            path_after: Some(path),
            operation: Some(CodeFileOperation::Modify),
            explicitly_edited: true,
        }
    }

    pub fn create(path: impl Into<String>) -> Self {
        Self {
            path_before: None,
            path_after: Some(path.into()),
            operation: Some(CodeFileOperation::Create),
            explicitly_edited: true,
        }
    }

    pub fn delete(path: impl Into<String>) -> Self {
        Self {
            path_before: Some(path.into()),
            path_after: None,
            operation: Some(CodeFileOperation::Delete),
            explicitly_edited: true,
        }
    }

    pub fn rename(path_before: impl Into<String>, path_after: impl Into<String>) -> Self {
        Self {
            path_before: Some(path_before.into()),
            path_after: Some(path_after.into()),
            operation: Some(CodeFileOperation::Rename),
            explicitly_edited: true,
        }
    }

    pub fn symlink_change(path: impl Into<String>) -> Self {
        let path = path.into();
        Self {
            path_before: Some(path.clone()),
            path_after: Some(path),
            operation: Some(CodeFileOperation::SymlinkChange),
            explicitly_edited: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeRollbackCaptureHandle {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub change_group_id: String,
    pub before_snapshot_id: String,
    targets: Vec<CodeRollbackCaptureTarget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedCodeChangeGroup {
    pub project_id: String,
    pub agent_session_id: String,
    pub run_id: String,
    pub change_group_id: String,
    pub before_snapshot_id: String,
    pub after_snapshot_id: String,
    pub file_version_count: usize,
    pub affected_files: Vec<CompletedCodeChangeFile>,
    pub history_metadata: Option<CodeChangeGroupHistoryMetadataRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedCodeChangeFile {
    pub path_before: Option<String>,
    pub path_after: Option<String>,
    pub operation: CodeFileOperation,
    pub before_hash: Option<String>,
    pub after_hash: Option<String>,
    pub explicitly_edited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
struct CodeSnapshotRestoreOutcome {
    pub snapshot_id: String,
    pub restored_paths: Vec<String>,
    pub removed_paths: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CodeRollbackRetentionPolicy {
    pub min_unreferenced_age_seconds: i64,
}

impl Default for CodeRollbackRetentionPolicy {
    fn default() -> Self {
        Self {
            min_unreferenced_age_seconds: DEFAULT_UNREFERENCED_BLOB_RETENTION_SECONDS,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeRollbackPruneReport {
    pub project_id: String,
    pub scanned_blob_count: usize,
    pub reachable_blob_count: usize,
    pub pruned_blob_count: usize,
    pub retained_unreferenced_blob_count: usize,
    pub missing_blob_file_count: usize,
    pub pruned_bytes: u64,
    pub diagnostics: Vec<CodeRollbackStorageDiagnostic>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeRollbackMaintenanceReport {
    pub project_id: String,
    pub generated_at: String,
    pub diagnostics: Vec<CodeRollbackStorageDiagnostic>,
    pub prune: CodeRollbackPruneReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RollbackTargetChangeGroup {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    change_group_id: String,
    before_snapshot_id: String,
    summary_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeHistoryOperationStart {
    project_id: String,
    operation_id: String,
    mode: CodeHistoryOperationMode,
    target_kind: CodeHistoryOperationTargetKind,
    target_id: String,
    target_change_group_id: Option<String>,
    target_file_path: Option<String>,
    target_hunk_ids: Vec<String>,
    agent_session_id: Option<String>,
    run_id: Option<String>,
    expected_workspace_epoch: Option<u64>,
    affected_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExistingCodeHistoryOperation {
    operation_id: String,
    status: CodeHistoryOperationStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeHistoryOperationRecoveryRow {
    operation_id: String,
    status: CodeHistoryOperationStatus,
    result_change_group_id: Option<String>,
    result_commit_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeHistoryOperationCommitRecoveryRow {
    commit_id: String,
    change_group_id: String,
}

enum CodeHistoryOperationBegin {
    Started,
    Existing(ExistingCodeHistoryOperation),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CodeRollbackStorageDiagnostic {
    pub code: String,
    pub snapshot_id: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeBlobRow {
    blob_id: String,
    size_bytes: u64,
    storage_path: String,
    created_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CodeSnapshotScanBudget {
    max_walk_entries: usize,
    max_new_blob_bytes: u64,
}

impl Default for CodeSnapshotScanBudget {
    fn default() -> Self {
        Self {
            max_walk_entries: DEFAULT_SNAPSHOT_SCAN_MAX_WALK_ENTRIES,
            max_new_blob_bytes: DEFAULT_SNAPSHOT_SCAN_MAX_NEW_BLOB_BYTES,
        }
    }
}

#[derive(Debug, Default)]
struct CodeSnapshotScanProgress {
    walked_entries: usize,
    new_blob_bytes: u64,
}

impl CodeSnapshotScanProgress {
    fn try_visit_walk_entry(&mut self, budget: CodeSnapshotScanBudget) -> bool {
        if self.walked_entries >= budget.max_walk_entries {
            return false;
        }
        self.walked_entries = self.walked_entries.saturating_add(1);
        true
    }

    fn try_reserve_new_blob_bytes(
        &mut self,
        budget: CodeSnapshotScanBudget,
        byte_count: u64,
    ) -> bool {
        let Some(next) = self.new_blob_bytes.checked_add(byte_count) else {
            return false;
        };
        if next > budget.max_new_blob_bytes {
            return false;
        }
        self.new_blob_bytes = next;
        true
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CodePatchPayloadBudget {
    max_non_explicit_diff_files: usize,
    max_text_hunk_payload_bytes: usize,
}

impl Default for CodePatchPayloadBudget {
    fn default() -> Self {
        Self {
            max_non_explicit_diff_files: DEFAULT_BROAD_CAPTURE_NON_EXPLICIT_DIFF_FILE_LIMIT,
            max_text_hunk_payload_bytes: DEFAULT_TEXT_HUNK_PAYLOAD_BYTES,
        }
    }
}

pub fn code_rollback_storage_dir_for_repo(repo_root: &Path) -> PathBuf {
    project_app_data_dir_for_repo(repo_root).join(CODE_ROLLBACK_DIR)
}

pub fn begin_exact_path_capture(
    repo_root: &Path,
    input: CodeChangeGroupInput,
    targets: Vec<CodeRollbackCaptureTarget>,
) -> CommandResult<CodeRollbackCaptureHandle> {
    validate_change_group_input(&input)?;
    if targets.is_empty() {
        return Err(CommandError::invalid_request("targets"));
    }
    let targets = normalize_targets(targets)?;
    let change_group_id = input
        .change_group_id
        .clone()
        .unwrap_or_else(|| generate_id("code-change"));

    insert_change_group_open(repo_root, &input, &change_group_id)?;
    let explicit_paths = explicit_paths_for_targets(&targets);
    let before_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: input.project_id.clone(),
            agent_session_id: input.agent_session_id.clone(),
            run_id: input.run_id.clone(),
            change_group_id: Some(change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::Before,
            previous_snapshot_id: None,
            explicit_paths,
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ =
                mark_change_group_failed(repo_root, &input.project_id, &change_group_id, &error);
            return Err(error);
        }
    };
    update_change_group_before_snapshot(
        repo_root,
        &input.project_id,
        &change_group_id,
        &before_snapshot.snapshot_id,
    )?;

    Ok(CodeRollbackCaptureHandle {
        project_id: input.project_id,
        agent_session_id: input.agent_session_id,
        run_id: input.run_id,
        change_group_id,
        before_snapshot_id: before_snapshot.snapshot_id,
        targets,
    })
}

pub fn complete_exact_path_capture(
    repo_root: &Path,
    handle: CodeRollbackCaptureHandle,
) -> CommandResult<CompletedCodeChangeGroup> {
    let explicit_paths = explicit_paths_for_targets(&handle.targets);
    let after_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: handle.project_id.clone(),
            agent_session_id: handle.agent_session_id.clone(),
            run_id: handle.run_id.clone(),
            change_group_id: Some(handle.change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::After,
            previous_snapshot_id: Some(handle.before_snapshot_id.clone()),
            explicit_paths,
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = mark_change_group_failed(
                repo_root,
                &handle.project_id,
                &handle.change_group_id,
                &error,
            );
            return Err(error);
        }
    };

    persist_exact_file_versions(repo_root, &handle, &after_snapshot.snapshot_id)
}

pub fn begin_broad_capture(
    repo_root: &Path,
    input: CodeChangeGroupInput,
) -> CommandResult<CodeRollbackCaptureHandle> {
    begin_broad_capture_with_budget(repo_root, input, CodeSnapshotScanBudget::default())
}

fn begin_broad_capture_with_budget(
    repo_root: &Path,
    input: CodeChangeGroupInput,
    scan_budget: CodeSnapshotScanBudget,
) -> CommandResult<CodeRollbackCaptureHandle> {
    validate_change_group_input(&input)?;
    let change_group_id = input
        .change_group_id
        .clone()
        .unwrap_or_else(|| generate_id("code-change"));
    let explicit_paths = broad_capture_explicit_paths(repo_root, &input.project_id)?;
    insert_change_group_open(repo_root, &input, &change_group_id)?;
    let before_snapshot = match capture_code_snapshot_internal_with_budget(
        repo_root,
        SnapshotCaptureRequest {
            project_id: input.project_id.clone(),
            agent_session_id: input.agent_session_id.clone(),
            run_id: input.run_id.clone(),
            change_group_id: Some(change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::Before,
            previous_snapshot_id: None,
            explicit_paths,
        },
        scan_budget,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ =
                mark_change_group_failed(repo_root, &input.project_id, &change_group_id, &error);
            return Err(error);
        }
    };
    update_change_group_before_snapshot(
        repo_root,
        &input.project_id,
        &change_group_id,
        &before_snapshot.snapshot_id,
    )?;

    Ok(CodeRollbackCaptureHandle {
        project_id: input.project_id,
        agent_session_id: input.agent_session_id,
        run_id: input.run_id,
        change_group_id,
        before_snapshot_id: before_snapshot.snapshot_id,
        targets: Vec::new(),
    })
}

pub fn complete_broad_capture(
    repo_root: &Path,
    handle: CodeRollbackCaptureHandle,
) -> CommandResult<CompletedCodeChangeGroup> {
    complete_broad_capture_with_budgets(
        repo_root,
        handle,
        CodeSnapshotScanBudget::default(),
        CodePatchPayloadBudget::default(),
    )
}

fn complete_broad_capture_with_budgets(
    repo_root: &Path,
    handle: CodeRollbackCaptureHandle,
    scan_budget: CodeSnapshotScanBudget,
    patch_budget: CodePatchPayloadBudget,
) -> CommandResult<CompletedCodeChangeGroup> {
    let explicit_paths = broad_capture_explicit_paths(repo_root, &handle.project_id)?;
    let after_snapshot = match capture_code_snapshot_internal_with_budget(
        repo_root,
        SnapshotCaptureRequest {
            project_id: handle.project_id.clone(),
            agent_session_id: handle.agent_session_id.clone(),
            run_id: handle.run_id.clone(),
            change_group_id: Some(handle.change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::After,
            previous_snapshot_id: Some(handle.before_snapshot_id.clone()),
            explicit_paths: explicit_paths.clone(),
        },
        scan_budget,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = mark_change_group_failed(
                repo_root,
                &handle.project_id,
                &handle.change_group_id,
                &error,
            );
            return Err(error);
        }
    };

    persist_broad_file_versions(
        repo_root,
        &handle,
        &after_snapshot.snapshot_id,
        &explicit_paths,
        patch_budget,
    )
}

pub fn fail_code_change_capture(
    repo_root: &Path,
    handle: &CodeRollbackCaptureHandle,
    error: &CommandError,
) -> CommandResult<()> {
    mark_change_group_failed(
        repo_root,
        &handle.project_id,
        &handle.change_group_id,
        error,
    )
}

#[cfg(test)]
fn restore_code_snapshot(
    repo_root: &Path,
    project_id: &str,
    snapshot_id: &str,
) -> CommandResult<CodeSnapshotRestoreOutcome> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(snapshot_id, "snapshotId")?;

    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let manifest =
        load_completed_snapshot_manifest(&connection, &database_path, project_id, snapshot_id)?;
    let blob_bytes = load_required_blob_bytes(&connection, repo_root, project_id, &manifest)?;
    drop(connection);

    apply_manifest_to_project(repo_root, snapshot_id, &manifest, &blob_bytes)
}

pub fn apply_code_rollback(
    repo_root: &Path,
    project_id: &str,
    target_change_group_id: &str,
) -> CommandResult<AppliedCodeRollback> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(target_change_group_id, "targetChangeGroupId")?;

    let target = load_rollback_target_change_group(repo_root, project_id, target_change_group_id)?;
    let operation_id = generate_id("code-rollback");
    insert_pending_rollback_operation(repo_root, &operation_id, &target, None)?;

    let applied = match apply_code_change_group_undo(
        repo_root,
        ApplyCodeChangeGroupUndoRequest {
            project_id: target.project_id.clone(),
            operation_id: Some(operation_id.clone()),
            target_change_group_id: target.change_group_id.clone(),
            expected_workspace_epoch: None,
        },
    ) {
        Ok(applied) => applied,
        Err(error) => {
            mark_rollback_operation_failed_best_effort(
                repo_root,
                &target.project_id,
                &operation_id,
                &error,
            );
            return Err(error);
        }
    };

    if applied.status == CodeFileUndoApplyStatus::Conflicted {
        let error = code_rollback_conflict_error(&target.change_group_id, &applied.conflicts);
        mark_rollback_operation_failed_best_effort(
            repo_root,
            &target.project_id,
            &operation_id,
            &error,
        );
        return Err(error);
    }

    applied_code_rollback_from_change_group_undo(repo_root, target, applied)
}

pub fn apply_code_file_undo(
    repo_root: &Path,
    request: ApplyCodeFileUndoRequest,
) -> CommandResult<AppliedCodeFileUndo> {
    validate_apply_code_file_undo_request(&request)?;

    let restore_lock = project_restore_lock(repo_root, &request.project_id)?;
    let _restore_guard = restore_lock.lock().map_err(|_| {
        CommandError::system_fault(
            "code_undo_lock_poisoned",
            "Xero could not enter the code undo apply lane.",
        )
    })?;

    let operation_id = request
        .operation_id
        .clone()
        .unwrap_or_else(|| generate_id("code-undo"));
    let target_commit = load_code_undo_target_commit(
        repo_root,
        &request.project_id,
        &request.target_change_group_id,
    )?;
    let parent_head = ensure_code_workspace_head(repo_root, &request.project_id)?;
    let target_file = selected_patch_file_for_undo(&target_commit.files, &request)?.clone();
    let selected_target_file =
        patch_file_with_selected_hunks(&target_file, &request.target_hunk_ids)?;
    let affected_paths = affected_paths_for_one_patch_file(&target_file);
    let target_file_path = patch_file_result_path(&target_file);
    match begin_code_history_operation(
        repo_root,
        &CodeHistoryOperationStart {
            project_id: request.project_id.clone(),
            operation_id: operation_id.clone(),
            mode: CodeHistoryOperationMode::SelectiveUndo,
            target_kind: file_undo_history_target_kind(&request),
            target_id: file_undo_history_target_id(&request, &target_file),
            target_change_group_id: Some(request.target_change_group_id.clone()),
            target_file_path: Some(target_file_path.clone()),
            target_hunk_ids: request.target_hunk_ids.clone(),
            agent_session_id: Some(target_commit.commit.agent_session_id.clone()),
            run_id: Some(target_commit.commit.run_id.clone()),
            expected_workspace_epoch: request.expected_workspace_epoch,
            affected_paths: affected_paths.clone(),
        },
    )? {
        CodeHistoryOperationBegin::Started => {}
        CodeHistoryOperationBegin::Existing(existing) => {
            return Err(reject_existing_code_history_operation(existing));
        }
    }

    if let Some(expected_epoch) = request.expected_workspace_epoch {
        if parent_head.workspace_epoch != expected_epoch {
            let conflicts = vec![CodeFileUndoConflict {
                path: patch_file_result_path(&target_file),
                kind: CodeFileUndoConflictKind::StaleWorkspace,
                message: format!(
                    "The workspace is at epoch {}, but this undo expected epoch {expected_epoch}. Refresh the current files before undoing this change.",
                    parent_head.workspace_epoch
                ),
                base_hash: None,
                selected_hash: None,
                current_hash: None,
                hunk_ids: request.target_hunk_ids.clone(),
            }];
            mark_code_history_operation_conflicted(
                repo_root,
                &request.project_id,
                &operation_id,
                &affected_paths,
                &conflicts,
            )?;
            return Ok(conflicted_code_file_undo_result(
                &request,
                &operation_id,
                &target_commit.commit.agent_session_id,
                &target_commit.commit.run_id,
                &target_file,
                affected_paths,
                conflicts,
                Some(parent_head),
                Some(code_change_group_history_metadata_from_commit(
                    &target_commit,
                )),
            ));
        }
    }

    let plan = match plan_selected_file_undo(repo_root, &request.project_id, &selected_target_file)
    {
        Ok(plan) => plan,
        Err(error) => {
            mark_code_history_operation_failed_best_effort(
                repo_root,
                &request.project_id,
                &operation_id,
                &error,
            );
            return Err(error);
        }
    };
    if !plan.conflicts.is_empty() {
        mark_code_history_operation_conflicted(
            repo_root,
            &request.project_id,
            &operation_id,
            &affected_paths,
            &plan.conflicts,
        )?;
        return Ok(conflicted_code_file_undo_result(
            &request,
            &operation_id,
            &target_commit.commit.agent_session_id,
            &target_commit.commit.run_id,
            &selected_target_file,
            affected_paths,
            plan.conflicts,
            Some(parent_head),
            Some(code_change_group_history_metadata_from_commit(
                &target_commit,
            )),
        ));
    }

    let pre_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: request.project_id.clone(),
            agent_session_id: target_commit.commit.agent_session_id.clone(),
            run_id: target_commit.commit.run_id.clone(),
            change_group_id: None,
            boundary_kind: CodeSnapshotBoundaryKind::PreRollback,
            previous_snapshot_id: None,
            explicit_paths: affected_paths.iter().cloned().collect(),
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            mark_code_history_operation_failed_best_effort(
                repo_root,
                &request.project_id,
                &operation_id,
                &error,
            );
            return Err(error);
        }
    };

    let result_change_group_id = generate_id("code-change");
    let result_group_input = CodeChangeGroupInput {
        project_id: request.project_id.clone(),
        agent_session_id: target_commit.commit.agent_session_id.clone(),
        run_id: target_commit.commit.run_id.clone(),
        change_group_id: Some(result_change_group_id.clone()),
        parent_change_group_id: Some(request.target_change_group_id.clone()),
        tool_call_id: None,
        runtime_event_id: None,
        conversation_sequence: None,
        change_kind: CodeChangeKind::Rollback,
        summary_label: format!("Undo {}", target_commit.commit.summary_label),
        restore_state: CodeChangeRestoreState::SnapshotAvailable,
    };
    if let Err(error) =
        insert_change_group_open(repo_root, &result_group_input, &result_change_group_id)
    {
        mark_code_history_operation_failed_best_effort(
            repo_root,
            &request.project_id,
            &operation_id,
            &error,
        );
        return Err(error);
    }
    if let Err(error) = update_change_group_before_snapshot(
        repo_root,
        &request.project_id,
        &result_change_group_id,
        &pre_snapshot.snapshot_id,
    ) {
        mark_code_history_operation_failed_best_effort(
            repo_root,
            &request.project_id,
            &operation_id,
            &error,
        );
        return Err(error);
    }
    mark_code_history_operation_applying(
        repo_root,
        &request.project_id,
        &operation_id,
        &result_change_group_id,
        &affected_paths,
    )?;

    if let Err(error) = apply_selected_file_undo_plan(repo_root, &request.project_id, &plan) {
        let _ = mark_change_group_failed(
            repo_root,
            &request.project_id,
            &result_change_group_id,
            &error,
        );
        mark_code_history_operation_repair_needed_best_effort(
            repo_root,
            &request.project_id,
            &operation_id,
            "code_history_operation_apply_interrupted",
            format!(
                "Undo operation `{operation_id}` failed while applying file changes. Inspect the workspace before retrying."
            ),
        );
        return Err(error);
    }

    let post_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: request.project_id.clone(),
            agent_session_id: target_commit.commit.agent_session_id.clone(),
            run_id: target_commit.commit.run_id.clone(),
            change_group_id: Some(result_change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::PostRollback,
            previous_snapshot_id: Some(pre_snapshot.snapshot_id.clone()),
            explicit_paths: affected_paths.iter().cloned().collect(),
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = mark_change_group_failed(
                repo_root,
                &request.project_id,
                &result_change_group_id,
                &error,
            );
            mark_code_history_operation_repair_needed_best_effort(
                repo_root,
                &request.project_id,
                &operation_id,
                "code_history_operation_post_snapshot_failed",
                format!(
                    "Undo operation `{operation_id}` changed files but could not capture its post-undo snapshot."
                ),
            );
            return Err(error);
        }
    };

    let applied = match persist_code_undo_history(
        repo_root,
        &request.project_id,
        &operation_id,
        &target_commit,
        std::slice::from_ref(&selected_target_file),
        &parent_head,
        &result_change_group_id,
        &pre_snapshot,
        &post_snapshot,
    ) {
        Ok(applied) => applied,
        Err(error) => {
            let _ = mark_change_group_failed(
                repo_root,
                &request.project_id,
                &result_change_group_id,
                &error,
            );
            mark_code_history_operation_repair_needed_best_effort(
                repo_root,
                &request.project_id,
                &operation_id,
                "code_history_operation_persist_failed",
                format!(
                    "Undo operation `{operation_id}` changed files but could not finish its history commit."
                ),
            );
            return Err(error);
        }
    };

    let result_commit_id = applied
        .history_metadata
        .as_ref()
        .and_then(|metadata| metadata.commit_id.clone());
    mark_code_history_operation_completed(
        repo_root,
        &request.project_id,
        &operation_id,
        Some(&result_change_group_id),
        result_commit_id.as_deref(),
        &affected_paths,
    )?;

    Ok(AppliedCodeFileUndo {
        project_id: request.project_id,
        agent_session_id: target_commit.commit.agent_session_id,
        run_id: target_commit.commit.run_id,
        operation_id,
        status: CodeFileUndoApplyStatus::Completed,
        target_change_group_id: request.target_change_group_id,
        target_patch_file_id: target_file.patch_file_id,
        target_file_path,
        selected_hunk_ids: request.target_hunk_ids,
        result_change_group_id: Some(result_change_group_id),
        result_commit_id,
        affected_paths,
        conflicts: Vec::new(),
        workspace_head: applied.workspace_head,
        patch_availability: applied.history_metadata,
        affected_files: applied.affected_files,
    })
}

pub fn apply_code_change_group_undo(
    repo_root: &Path,
    request: ApplyCodeChangeGroupUndoRequest,
) -> CommandResult<AppliedCodeChangeGroupUndo> {
    validate_apply_code_change_group_undo_request(&request)?;

    let restore_lock = project_restore_lock(repo_root, &request.project_id)?;
    let _restore_guard = restore_lock.lock().map_err(|_| {
        CommandError::system_fault(
            "code_undo_lock_poisoned",
            "Xero could not enter the code undo apply lane.",
        )
    })?;

    let operation_id = request
        .operation_id
        .clone()
        .unwrap_or_else(|| generate_id("code-undo"));
    let target_commit = load_code_undo_target_commit(
        repo_root,
        &request.project_id,
        &request.target_change_group_id,
    )?;
    if target_commit.files.is_empty() {
        return Err(CommandError::user_fixable(
            "code_undo_patch_unavailable",
            format!(
                "Code change group `{}` does not contain any replayable file changes.",
                request.target_change_group_id
            ),
        ));
    }

    let parent_head = ensure_code_workspace_head(repo_root, &request.project_id)?;
    let affected_paths = affected_paths_for_patch_records(&target_commit.files);
    match begin_code_history_operation(
        repo_root,
        &CodeHistoryOperationStart {
            project_id: request.project_id.clone(),
            operation_id: operation_id.clone(),
            mode: CodeHistoryOperationMode::SelectiveUndo,
            target_kind: CodeHistoryOperationTargetKind::ChangeGroup,
            target_id: request.target_change_group_id.clone(),
            target_change_group_id: Some(request.target_change_group_id.clone()),
            target_file_path: None,
            target_hunk_ids: Vec::new(),
            agent_session_id: Some(target_commit.commit.agent_session_id.clone()),
            run_id: Some(target_commit.commit.run_id.clone()),
            expected_workspace_epoch: request.expected_workspace_epoch,
            affected_paths: affected_paths.clone(),
        },
    )? {
        CodeHistoryOperationBegin::Started => {}
        CodeHistoryOperationBegin::Existing(existing) => {
            return Err(reject_existing_code_history_operation(existing));
        }
    }

    if let Some(expected_epoch) = request.expected_workspace_epoch {
        if parent_head.workspace_epoch != expected_epoch {
            let conflicts = vec![CodeFileUndoConflict {
                path: target_commit
                    .files
                    .first()
                    .map(patch_file_result_path)
                    .unwrap_or_else(|| request.target_change_group_id.clone()),
                kind: CodeFileUndoConflictKind::StaleWorkspace,
                message: format!(
                    "The workspace is at epoch {}, but this undo expected epoch {expected_epoch}. Refresh the current files before undoing this change group.",
                    parent_head.workspace_epoch
                ),
                base_hash: None,
                selected_hash: None,
                current_hash: None,
                hunk_ids: Vec::new(),
            }];
            mark_code_history_operation_conflicted(
                repo_root,
                &request.project_id,
                &operation_id,
                &affected_paths,
                &conflicts,
            )?;
            return Ok(conflicted_code_change_group_undo_result(
                &request,
                &operation_id,
                &target_commit.commit.agent_session_id,
                &target_commit.commit.run_id,
                affected_paths,
                conflicts,
                Some(parent_head),
                Some(code_change_group_history_metadata_from_commit(
                    &target_commit,
                )),
            ));
        }
    }

    let plan = match plan_change_group_undo(repo_root, &request.project_id, &target_commit.files) {
        Ok(plan) => plan,
        Err(error) => {
            mark_code_history_operation_failed_best_effort(
                repo_root,
                &request.project_id,
                &operation_id,
                &error,
            );
            return Err(error);
        }
    };
    if !plan.conflicts.is_empty() {
        mark_code_history_operation_conflicted(
            repo_root,
            &request.project_id,
            &operation_id,
            &affected_paths,
            &plan.conflicts,
        )?;
        return Ok(conflicted_code_change_group_undo_result(
            &request,
            &operation_id,
            &target_commit.commit.agent_session_id,
            &target_commit.commit.run_id,
            affected_paths,
            plan.conflicts,
            Some(parent_head),
            Some(code_change_group_history_metadata_from_commit(
                &target_commit,
            )),
        ));
    }

    let pre_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: request.project_id.clone(),
            agent_session_id: target_commit.commit.agent_session_id.clone(),
            run_id: target_commit.commit.run_id.clone(),
            change_group_id: None,
            boundary_kind: CodeSnapshotBoundaryKind::PreRollback,
            previous_snapshot_id: None,
            explicit_paths: affected_paths.iter().cloned().collect(),
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            mark_code_history_operation_failed_best_effort(
                repo_root,
                &request.project_id,
                &operation_id,
                &error,
            );
            return Err(error);
        }
    };

    let result_change_group_id = generate_id("code-change");
    let result_group_input = CodeChangeGroupInput {
        project_id: request.project_id.clone(),
        agent_session_id: target_commit.commit.agent_session_id.clone(),
        run_id: target_commit.commit.run_id.clone(),
        change_group_id: Some(result_change_group_id.clone()),
        parent_change_group_id: Some(request.target_change_group_id.clone()),
        tool_call_id: None,
        runtime_event_id: None,
        conversation_sequence: None,
        change_kind: CodeChangeKind::Rollback,
        summary_label: format!("Undo {}", target_commit.commit.summary_label),
        restore_state: CodeChangeRestoreState::SnapshotAvailable,
    };
    if let Err(error) =
        insert_change_group_open(repo_root, &result_group_input, &result_change_group_id)
    {
        mark_code_history_operation_failed_best_effort(
            repo_root,
            &request.project_id,
            &operation_id,
            &error,
        );
        return Err(error);
    }
    if let Err(error) = update_change_group_before_snapshot(
        repo_root,
        &request.project_id,
        &result_change_group_id,
        &pre_snapshot.snapshot_id,
    ) {
        mark_code_history_operation_failed_best_effort(
            repo_root,
            &request.project_id,
            &operation_id,
            &error,
        );
        return Err(error);
    }
    mark_code_history_operation_applying(
        repo_root,
        &request.project_id,
        &operation_id,
        &result_change_group_id,
        &affected_paths,
    )?;

    if let Err(error) =
        apply_selected_file_undo_plans(repo_root, &request.project_id, &plan.file_plans)
    {
        let _ = mark_change_group_failed(
            repo_root,
            &request.project_id,
            &result_change_group_id,
            &error,
        );
        mark_code_history_operation_repair_needed_best_effort(
            repo_root,
            &request.project_id,
            &operation_id,
            "code_history_operation_apply_interrupted",
            format!(
                "Undo operation `{operation_id}` failed while applying file changes. Inspect the workspace before retrying."
            ),
        );
        return Err(error);
    }

    let post_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: request.project_id.clone(),
            agent_session_id: target_commit.commit.agent_session_id.clone(),
            run_id: target_commit.commit.run_id.clone(),
            change_group_id: Some(result_change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::PostRollback,
            previous_snapshot_id: Some(pre_snapshot.snapshot_id.clone()),
            explicit_paths: affected_paths.iter().cloned().collect(),
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ = mark_change_group_failed(
                repo_root,
                &request.project_id,
                &result_change_group_id,
                &error,
            );
            mark_code_history_operation_repair_needed_best_effort(
                repo_root,
                &request.project_id,
                &operation_id,
                "code_history_operation_post_snapshot_failed",
                format!(
                    "Undo operation `{operation_id}` changed files but could not capture its post-undo snapshot."
                ),
            );
            return Err(error);
        }
    };

    let applied = match persist_code_undo_history(
        repo_root,
        &request.project_id,
        &operation_id,
        &target_commit,
        &target_commit.files,
        &parent_head,
        &result_change_group_id,
        &pre_snapshot,
        &post_snapshot,
    ) {
        Ok(applied) => applied,
        Err(error) => {
            let _ = mark_change_group_failed(
                repo_root,
                &request.project_id,
                &result_change_group_id,
                &error,
            );
            mark_code_history_operation_repair_needed_best_effort(
                repo_root,
                &request.project_id,
                &operation_id,
                "code_history_operation_persist_failed",
                format!(
                    "Undo operation `{operation_id}` changed files but could not finish its history commit."
                ),
            );
            return Err(error);
        }
    };

    let result_commit_id = applied
        .history_metadata
        .as_ref()
        .and_then(|metadata| metadata.commit_id.clone());
    mark_code_history_operation_completed(
        repo_root,
        &request.project_id,
        &operation_id,
        Some(&result_change_group_id),
        result_commit_id.as_deref(),
        &affected_paths,
    )?;

    Ok(AppliedCodeChangeGroupUndo {
        project_id: request.project_id,
        agent_session_id: target_commit.commit.agent_session_id,
        run_id: target_commit.commit.run_id,
        operation_id,
        status: CodeFileUndoApplyStatus::Completed,
        target_change_group_id: request.target_change_group_id,
        result_change_group_id: Some(result_change_group_id),
        result_commit_id,
        affected_paths,
        conflicts: Vec::new(),
        workspace_head: applied.workspace_head,
        patch_availability: applied.history_metadata,
        affected_files: applied.affected_files,
    })
}

pub fn apply_code_session_rollback(
    repo_root: &Path,
    request: ApplyCodeSessionRollbackRequest,
) -> CommandResult<AppliedCodeSessionRollback> {
    validate_apply_code_session_rollback_request(&request)?;

    let project_id = request.boundary.project_id.clone();
    let restore_lock = project_restore_lock(repo_root, &project_id)?;
    let _restore_guard = restore_lock.lock().map_err(|_| {
        CommandError::system_fault(
            "code_session_rollback_lock_poisoned",
            "Xero could not enter the code session rollback apply lane.",
        )
    })?;

    let operation_id = request
        .operation_id
        .clone()
        .unwrap_or_else(|| generate_id("code-session-rollback"));
    let parent_head = ensure_code_workspace_head(repo_root, &project_id)?;
    let plan = build_code_session_lineage_undo_plan(
        repo_root,
        &BuildCodeSessionLineageUndoPlanRequest {
            boundary: request.boundary.clone(),
            explicitly_selected_change_group_ids: request
                .explicitly_selected_change_group_ids
                .clone(),
        },
    )?;
    let affected_paths = plan.affected_paths.clone();
    let target_change_group_ids = session_rollback_target_change_group_ids(&plan);

    if target_change_group_ids.is_empty() || affected_paths.is_empty() {
        return Err(CommandError::user_fixable(
            "code_session_rollback_empty",
            "There are no later code changes in this session lineage to return from.",
        ));
    }

    match begin_code_history_operation(
        repo_root,
        &CodeHistoryOperationStart {
            project_id: project_id.clone(),
            operation_id: operation_id.clone(),
            mode: CodeHistoryOperationMode::SessionRollback,
            target_kind: session_rollback_history_target_kind(request.boundary.target_kind),
            target_id: request.boundary.target_id.clone(),
            target_change_group_id: plan.boundary.boundary_change_group_id.clone(),
            target_file_path: None,
            target_hunk_ids: Vec::new(),
            agent_session_id: Some(plan.agent_session_id.clone()),
            run_id: Some(plan.boundary.boundary_run_id.clone()),
            expected_workspace_epoch: request.expected_workspace_epoch,
            affected_paths: affected_paths.clone(),
        },
    )? {
        CodeHistoryOperationBegin::Started => {}
        CodeHistoryOperationBegin::Existing(existing) => {
            return Err(reject_existing_code_history_operation(existing));
        }
    }

    if let Some(expected_epoch) = request.expected_workspace_epoch {
        if parent_head.workspace_epoch != expected_epoch {
            let conflicts = vec![CodeFileUndoConflict {
                path: plan
                    .affected_paths
                    .first()
                    .cloned()
                    .unwrap_or_else(|| request.boundary.target_id.clone()),
                kind: CodeFileUndoConflictKind::StaleWorkspace,
                message: format!(
                    "The workspace is at epoch {}, but this session rollback expected epoch {expected_epoch}. Refresh the current files before returning this session to the selected boundary.",
                    parent_head.workspace_epoch
                ),
                base_hash: None,
                selected_hash: None,
                current_hash: None,
                hunk_ids: Vec::new(),
            }];
            mark_code_history_operation_conflicted(
                repo_root,
                &project_id,
                &operation_id,
                &affected_paths,
                &conflicts,
            )?;
            return Ok(conflicted_code_session_rollback_result(
                &request,
                &operation_id,
                &plan,
                affected_paths,
                conflicts,
                Some(parent_head),
                None,
            ));
        }
    }

    if plan.status == CodeSessionLineageUndoPlanStatus::Conflicted {
        let conflicts = plan
            .conflicts
            .iter()
            .map(code_file_undo_conflict_from_session_lineage)
            .collect::<Vec<_>>();
        mark_code_history_operation_conflicted(
            repo_root,
            &project_id,
            &operation_id,
            &affected_paths,
            &conflicts,
        )?;
        return Ok(conflicted_code_session_rollback_result(
            &request,
            &operation_id,
            &plan,
            affected_paths,
            conflicts,
            Some(parent_head),
            None,
        ));
    }

    let pre_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: project_id.clone(),
            agent_session_id: plan.agent_session_id.clone(),
            run_id: plan.boundary.boundary_run_id.clone(),
            change_group_id: None,
            boundary_kind: CodeSnapshotBoundaryKind::PreRollback,
            previous_snapshot_id: None,
            explicit_paths: affected_paths.iter().cloned().collect(),
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            mark_code_history_operation_failed_best_effort(
                repo_root,
                &project_id,
                &operation_id,
                &error,
            );
            return Err(error);
        }
    };

    let result_change_group_id = generate_id("code-change");
    let result_group_input = CodeChangeGroupInput {
        project_id: project_id.clone(),
        agent_session_id: plan.agent_session_id.clone(),
        run_id: plan.boundary.boundary_run_id.clone(),
        change_group_id: Some(result_change_group_id.clone()),
        parent_change_group_id: plan.boundary.boundary_change_group_id.clone(),
        tool_call_id: None,
        runtime_event_id: None,
        conversation_sequence: None,
        change_kind: CodeChangeKind::Rollback,
        summary_label: session_rollback_summary_label(&plan),
        restore_state: CodeChangeRestoreState::SnapshotAvailable,
    };
    if let Err(error) =
        insert_change_group_open(repo_root, &result_group_input, &result_change_group_id)
    {
        mark_code_history_operation_failed_best_effort(
            repo_root,
            &project_id,
            &operation_id,
            &error,
        );
        return Err(error);
    }
    if let Err(error) = update_change_group_before_snapshot(
        repo_root,
        &project_id,
        &result_change_group_id,
        &pre_snapshot.snapshot_id,
    ) {
        mark_code_history_operation_failed_best_effort(
            repo_root,
            &project_id,
            &operation_id,
            &error,
        );
        return Err(error);
    }
    mark_code_history_operation_applying(
        repo_root,
        &project_id,
        &operation_id,
        &result_change_group_id,
        &affected_paths,
    )?;

    let file_plans = match selected_file_undo_plans_from_session_rollback_plan(&plan) {
        Ok(file_plans) => file_plans,
        Err(error) => {
            mark_code_history_operation_failed_best_effort(
                repo_root,
                &project_id,
                &operation_id,
                &error,
            );
            return Err(error);
        }
    };
    if let Err(error) = apply_selected_file_undo_plans(repo_root, &project_id, &file_plans) {
        let _ = mark_change_group_failed(repo_root, &project_id, &result_change_group_id, &error);
        mark_code_history_operation_repair_needed_best_effort(
            repo_root,
            &project_id,
            &operation_id,
            "code_history_operation_apply_interrupted",
            format!(
                "Session rollback operation `{operation_id}` failed while applying file changes. Inspect the workspace before retrying."
            ),
        );
        return Err(error);
    }

    let post_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: project_id.clone(),
            agent_session_id: plan.agent_session_id.clone(),
            run_id: plan.boundary.boundary_run_id.clone(),
            change_group_id: Some(result_change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::PostRollback,
            previous_snapshot_id: Some(pre_snapshot.snapshot_id.clone()),
            explicit_paths: affected_paths.iter().cloned().collect(),
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            let _ =
                mark_change_group_failed(repo_root, &project_id, &result_change_group_id, &error);
            mark_code_history_operation_repair_needed_best_effort(
                repo_root,
                &project_id,
                &operation_id,
                "code_history_operation_post_snapshot_failed",
                format!(
                    "Session rollback operation `{operation_id}` changed files but could not capture its post-rollback snapshot."
                ),
            );
            return Err(error);
        }
    };

    let applied = match persist_code_session_rollback_history(
        repo_root,
        &project_id,
        &operation_id,
        &plan,
        &parent_head,
        &result_change_group_id,
        &pre_snapshot,
        &post_snapshot,
    ) {
        Ok(applied) => applied,
        Err(error) => {
            let _ =
                mark_change_group_failed(repo_root, &project_id, &result_change_group_id, &error);
            mark_code_history_operation_repair_needed_best_effort(
                repo_root,
                &project_id,
                &operation_id,
                "code_history_operation_persist_failed",
                format!(
                    "Session rollback operation `{operation_id}` changed files but could not finish its history commit."
                ),
            );
            return Err(error);
        }
    };

    let result_commit_id = applied
        .history_metadata
        .as_ref()
        .and_then(|metadata| metadata.commit_id.clone());
    mark_code_history_operation_completed(
        repo_root,
        &project_id,
        &operation_id,
        Some(&result_change_group_id),
        result_commit_id.as_deref(),
        &affected_paths,
    )?;

    Ok(AppliedCodeSessionRollback {
        project_id,
        agent_session_id: plan.agent_session_id,
        run_id: plan.boundary.boundary_run_id,
        operation_id,
        status: CodeFileUndoApplyStatus::Completed,
        target_id: request.boundary.target_id,
        boundary_id: request.boundary.boundary_id,
        boundary_change_group_id: plan.boundary.boundary_change_group_id,
        target_change_group_ids,
        result_change_group_id: Some(result_change_group_id),
        result_commit_id,
        affected_paths,
        conflicts: Vec::new(),
        workspace_head: applied.workspace_head,
        patch_availability: applied.history_metadata,
        affected_files: applied.affected_files,
    })
}

#[derive(Debug)]
struct SelectedFileUndoPlan {
    text_content: Option<(String, String)>,
    file_actions: Vec<CodeFileOperationInverseAction>,
    conflicts: Vec<CodeFileUndoConflict>,
}

#[derive(Debug)]
struct ChangeGroupUndoPlan {
    file_plans: Vec<SelectedFileUndoPlan>,
    conflicts: Vec<CodeFileUndoConflict>,
}

#[derive(Debug)]
struct PersistedCodeUndo {
    history_metadata: Option<CodeChangeGroupHistoryMetadataRecord>,
    workspace_head: Option<CodeWorkspaceHeadRecord>,
    affected_files: Vec<CompletedCodeChangeFile>,
}

fn validate_apply_code_file_undo_request(request: &ApplyCodeFileUndoRequest) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.target_change_group_id, "targetChangeGroupId")?;
    if let Some(operation_id) = request.operation_id.as_deref() {
        validate_non_empty(operation_id, "operationId")?;
    }
    if let Some(patch_file_id) = request.target_patch_file_id.as_deref() {
        validate_non_empty(patch_file_id, "targetPatchFileId")?;
    }
    if let Some(path) = request.target_file_path.as_deref() {
        validate_non_empty(path, "targetFilePath")?;
        if safe_relative_path(path).is_none() {
            return Err(CommandError::invalid_request("targetFilePath"));
        }
    }
    let _ = normalize_selected_hunk_ids(&request.target_hunk_ids)?;
    if request.target_patch_file_id.is_none() && request.target_file_path.is_none() {
        return Err(CommandError::user_fixable(
            "code_undo_target_missing",
            "File undo must target a patch file id or a project-relative file path.",
        ));
    }
    Ok(())
}

fn normalize_selected_hunk_ids(hunk_ids: &[String]) -> CommandResult<Vec<String>> {
    let mut normalized = Vec::with_capacity(hunk_ids.len());
    let mut seen = BTreeSet::new();
    for hunk_id in hunk_ids {
        validate_non_empty(hunk_id, "targetHunkIds[]")?;
        if !seen.insert(hunk_id.clone()) {
            return Err(CommandError::user_fixable(
                "code_undo_hunk_duplicate",
                format!("Hunk id `{hunk_id}` was selected more than once."),
            ));
        }
        normalized.push(hunk_id.clone());
    }
    Ok(normalized)
}

fn validate_apply_code_change_group_undo_request(
    request: &ApplyCodeChangeGroupUndoRequest,
) -> CommandResult<()> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.target_change_group_id, "targetChangeGroupId")?;
    if let Some(operation_id) = request.operation_id.as_deref() {
        validate_non_empty(operation_id, "operationId")?;
    }
    Ok(())
}

fn validate_apply_code_session_rollback_request(
    request: &ApplyCodeSessionRollbackRequest,
) -> CommandResult<()> {
    validate_non_empty(&request.boundary.project_id, "projectId")?;
    validate_non_empty(&request.boundary.agent_session_id, "target.agentSessionId")?;
    validate_non_empty(&request.boundary.target_id, "target.targetId")?;
    validate_non_empty(&request.boundary.boundary_id, "target.boundaryId")?;
    if let Some(operation_id) = request.operation_id.as_deref() {
        validate_non_empty(operation_id, "operationId")?;
    }
    if request.boundary.target_kind == CodeSessionBoundaryTargetKind::RunBoundary
        && request.boundary.run_id.as_deref().is_none()
    {
        return Err(CommandError::invalid_request("target.runId"));
    }
    for change_group_id in &request.explicitly_selected_change_group_ids {
        validate_non_empty(change_group_id, "explicitlySelectedChangeGroupIds[]")?;
    }
    Ok(())
}

fn load_code_undo_target_commit(
    repo_root: &Path,
    project_id: &str,
    target_change_group_id: &str,
) -> CommandResult<CodePatchsetCommitRecord> {
    let metadata =
        read_code_change_group_history_metadata(repo_root, project_id, target_change_group_id)?
            .ok_or_else(|| {
                CommandError::user_fixable(
                    "code_undo_target_missing",
                    format!("Xero could not find code change group `{target_change_group_id}`."),
                )
            })?;
    let commit_id = metadata.commit_id.ok_or_else(|| {
        CommandError::user_fixable(
            "code_undo_patch_unavailable",
            metadata
                .patch_availability
                .unavailable_reason
                .unwrap_or_else(|| {
                    format!(
                        "Code change group `{target_change_group_id}` does not have a replayable patchset."
                    )
                }),
        )
    })?;
    read_code_patchset_commit(repo_root, project_id, &commit_id)?
        .ok_or_else(|| {
            CommandError::system_fault(
                "code_undo_commit_missing",
                format!(
                    "Code change group `{target_change_group_id}` references commit `{commit_id}`, but the commit is missing."
                ),
            )
        })
}

fn selected_patch_file_for_undo<'a>(
    files: &'a [CodePatchFileRecord],
    request: &ApplyCodeFileUndoRequest,
) -> CommandResult<&'a CodePatchFileRecord> {
    let matches = files
        .iter()
        .filter(|file| {
            request
                .target_patch_file_id
                .as_deref()
                .is_some_and(|patch_file_id| file.patch_file_id == patch_file_id)
                || request.target_file_path.as_deref().is_some_and(|path| {
                    file.path_before.as_deref() == Some(path)
                        || file.path_after.as_deref() == Some(path)
                })
        })
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [file] => Ok(*file),
        [] => Err(CommandError::user_fixable(
            "code_undo_file_missing",
            format!(
                "Xero could not find a patch file matching the selected undo target in change group `{}`.",
                request.target_change_group_id
            ),
        )),
        _ => Err(CommandError::user_fixable(
            "code_undo_file_ambiguous",
            "The selected undo path matches more than one patch file. Retry with a patch file id.",
        )),
    }
}

fn patch_file_with_selected_hunks(
    file: &CodePatchFileRecord,
    selected_hunk_ids: &[String],
) -> CommandResult<CodePatchFileRecord> {
    if selected_hunk_ids.is_empty() {
        return Ok(file.clone());
    }
    if file.operation != CodePatchFileOperation::Modify
        || file.merge_policy != CodePatchMergePolicy::Text
    {
        return Err(CommandError::user_fixable(
            "code_undo_hunks_unsupported",
            "Hunk-level undo can only target text modify patch files.",
        ));
    }

    let requested = normalize_selected_hunk_ids(selected_hunk_ids)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let available = file
        .hunks
        .iter()
        .map(|hunk| hunk.hunk_id.clone())
        .collect::<BTreeSet<_>>();
    let missing = requested
        .difference(&available)
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        return Err(CommandError::user_fixable(
            "code_undo_hunk_missing",
            format!(
                "The selected hunk id(s) are not present in this patch file: {}.",
                missing.join(", ")
            ),
        ));
    }

    let mut selected = file.clone();
    selected.hunks = file
        .hunks
        .iter()
        .filter(|hunk| requested.contains(&hunk.hunk_id))
        .cloned()
        .collect();
    selected.text_hunk_count = selected.hunks.len().min(u32::MAX as usize) as u32;
    Ok(selected)
}

fn plan_selected_file_undo(
    repo_root: &Path,
    project_id: &str,
    file: &CodePatchFileRecord,
) -> CommandResult<SelectedFileUndoPlan> {
    if file.operation == CodePatchFileOperation::Modify
        && file.merge_policy == CodePatchMergePolicy::Text
    {
        let current = read_current_text_for_patch_file(repo_root, file)?;
        let plan = plan_text_file_inverse_patch(file, current.as_deref());
        return Ok(match plan.status {
            CodeTextInversePatchPlanStatus::Clean => SelectedFileUndoPlan {
                text_content: plan
                    .planned_content
                    .map(|content| (plan.path.clone(), content)),
                file_actions: Vec::new(),
                conflicts: Vec::new(),
            },
            CodeTextInversePatchPlanStatus::Conflicted => SelectedFileUndoPlan {
                text_content: None,
                file_actions: Vec::new(),
                conflicts: plan
                    .conflicts
                    .iter()
                    .map(code_file_undo_conflict_from_text)
                    .collect(),
            },
        });
    }

    let current = current_file_operation_state(repo_root, project_id, file)?;
    let exact_file = exact_planner_file(file);
    let plan = plan_file_operation_inverse_patch(&exact_file, &current);
    Ok(match plan.status {
        CodeFileOperationInversePatchPlanStatus::Clean => SelectedFileUndoPlan {
            text_content: None,
            file_actions: plan.actions,
            conflicts: Vec::new(),
        },
        CodeFileOperationInversePatchPlanStatus::Conflicted => SelectedFileUndoPlan {
            text_content: None,
            file_actions: Vec::new(),
            conflicts: plan
                .conflicts
                .iter()
                .map(code_file_undo_conflict_from_file_operation)
                .collect(),
        },
    })
}

fn plan_change_group_undo(
    repo_root: &Path,
    project_id: &str,
    files: &[CodePatchFileRecord],
) -> CommandResult<ChangeGroupUndoPlan> {
    let mut file_plans = Vec::with_capacity(files.len());
    let mut conflicts = Vec::new();

    for file in files {
        let plan = plan_selected_file_undo(repo_root, project_id, file)?;
        if plan.conflicts.is_empty() {
            file_plans.push(plan);
        } else {
            conflicts.extend(plan.conflicts);
        }
    }

    if !conflicts.is_empty() {
        file_plans.clear();
    }

    Ok(ChangeGroupUndoPlan {
        file_plans,
        conflicts,
    })
}

fn read_current_text_for_patch_file(
    repo_root: &Path,
    file: &CodePatchFileRecord,
) -> CommandResult<Option<String>> {
    let path = patch_file_result_path(file);
    let absolute_path = absolute_repo_path(repo_root, &path)?;
    match fs::read(&absolute_path) {
        Ok(bytes) => String::from_utf8(bytes).map(Some).map_err(|error| {
            CommandError::user_fixable(
                "code_undo_text_decode_failed",
                format!(
                    "The current file `{path}` is not UTF-8 text, so the selected text change cannot be undone safely: {error}"
                ),
            )
        }),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(CommandError::retryable(
            "code_undo_text_read_failed",
            format!("Xero could not read `{path}` before undo planning: {error}"),
        )),
    }
}

fn current_file_operation_state(
    repo_root: &Path,
    project_id: &str,
    file: &CodePatchFileRecord,
) -> CommandResult<CodeFileOperationCurrentState> {
    let mut current = CodeFileOperationCurrentState::default();
    if let Some(path_before) = file.path_before.as_deref() {
        current.path_before = current_exact_state_for_path(repo_root, project_id, path_before)?;
    }
    if let Some(path_after) = file.path_after.as_deref() {
        if file.path_before.as_deref() == Some(path_after) {
            current.path_after = current.path_before.clone();
        } else {
            current.path_after = current_exact_state_for_path(repo_root, project_id, path_after)?;
        }
    }
    Ok(current)
}

fn current_exact_state_for_path(
    repo_root: &Path,
    project_id: &str,
    path: &str,
) -> CommandResult<Option<CodeExactFileState>> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    capture_manifest_entry(repo_root, &connection, project_id, path, None, true)
        .map(|entry| entry.map(|entry| exact_state_from_snapshot_entry(&entry)))
}

fn exact_planner_file(file: &CodePatchFileRecord) -> CodePatchFileRecord {
    if file.operation == CodePatchFileOperation::Modify {
        return file.clone();
    }
    let mut exact_file = file.clone();
    exact_file.merge_policy = CodePatchMergePolicy::Exact;
    exact_file
}

fn apply_selected_file_undo_plan(
    repo_root: &Path,
    project_id: &str,
    plan: &SelectedFileUndoPlan,
) -> CommandResult<()> {
    apply_selected_file_undo_plans(repo_root, project_id, std::slice::from_ref(plan))
}

fn apply_selected_file_undo_plans(
    repo_root: &Path,
    project_id: &str,
    plans: &[SelectedFileUndoPlan],
) -> CommandResult<()> {
    if plans.iter().any(|plan| !plan.conflicts.is_empty()) {
        return Err(CommandError::user_fixable(
            "code_undo_conflicted",
            "The selected undo has conflicts and cannot be applied.",
        ));
    }

    let file_actions = plans
        .iter()
        .flat_map(|plan| plan.file_actions.iter().cloned())
        .collect::<Vec<_>>();
    let blob_bytes = blob_bytes_for_inverse_actions(repo_root, project_id, &file_actions)?;

    for plan in plans {
        if let Some((path, content)) = &plan.text_content {
            write_text_file_atomically(repo_root, path, content)?;
        }
        for action in &plan.file_actions {
            apply_file_operation_inverse_action(repo_root, action, &blob_bytes)?;
        }
    }
    Ok(())
}

fn selected_file_undo_plans_from_session_rollback_plan(
    plan: &CodeSessionLineageUndoPlan,
) -> CommandResult<Vec<SelectedFileUndoPlan>> {
    plan.planned_files
        .iter()
        .map(selected_file_undo_plan_from_session_rollback_file)
        .collect()
}

fn selected_file_undo_plan_from_session_rollback_file(
    file: &CodeSessionLineageUndoPlanFile,
) -> CommandResult<SelectedFileUndoPlan> {
    let text_content = file
        .planned_content
        .clone()
        .map(|content| session_plan_file_result_path(file).map(|path| (path, content)))
        .transpose()?;

    Ok(SelectedFileUndoPlan {
        text_content,
        file_actions: file.file_actions.clone(),
        conflicts: Vec::new(),
    })
}

fn session_plan_file_result_path(file: &CodeSessionLineageUndoPlanFile) -> CommandResult<String> {
    file.path_after
        .as_deref()
        .or(file.path_before.as_deref())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            CommandError::system_fault(
                "code_session_rollback_plan_path_missing",
                format!(
                    "Session rollback planned file `{}` without a before or after path.",
                    file.patch_file_id
                ),
            )
        })
}

fn write_text_file_atomically(repo_root: &Path, path: &str, content: &str) -> CommandResult<()> {
    let absolute_path = absolute_repo_path(repo_root, path)?;
    if let Some(parent) = absolute_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "code_undo_write_prepare_failed",
                format!(
                    "Xero could not prepare {} for undo: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    let mode = fs::symlink_metadata(&absolute_path)
        .ok()
        .and_then(|metadata| {
            (!metadata.is_dir() || metadata.file_type().is_symlink()).then(|| file_mode(&metadata))
        })
        .flatten();
    let parent = absolute_path.parent().unwrap_or(repo_root);
    let temp_path = parent.join(format!(".{}.tmp", generate_id("code-undo")));
    fs::write(&temp_path, content).map_err(|error| {
        CommandError::retryable(
            "code_undo_write_failed",
            format!(
                "Xero could not write undo file {}: {error}",
                temp_path.display()
            ),
        )
    })?;
    set_mode_if_supported(&temp_path, mode)?;

    #[cfg(windows)]
    if absolute_path.exists() {
        let _ = fs::remove_file(&absolute_path);
    }

    fs::rename(&temp_path, &absolute_path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        CommandError::retryable(
            "code_undo_rename_failed",
            format!(
                "Xero could not finalize undo file {}: {error}",
                absolute_path.display()
            ),
        )
    })
}

fn blob_bytes_for_inverse_actions(
    repo_root: &Path,
    project_id: &str,
    actions: &[CodeFileOperationInverseAction],
) -> CommandResult<BTreeMap<String, Vec<u8>>> {
    let blob_ids = actions
        .iter()
        .filter_map(|action| action.restore_state.as_ref())
        .filter(|state| state.kind == CodePatchFileKind::File)
        .filter_map(|state| state.blob_id.clone())
        .collect::<BTreeSet<_>>();
    if blob_ids.is_empty() {
        return Ok(BTreeMap::new());
    }

    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let mut blobs = BTreeMap::new();
    for blob_id in blob_ids {
        let bytes = read_blob_bytes(&connection, repo_root, project_id, &blob_id)?;
        blobs.insert(blob_id, bytes);
    }
    Ok(blobs)
}

fn apply_file_operation_inverse_action(
    repo_root: &Path,
    action: &CodeFileOperationInverseAction,
    blob_bytes: &BTreeMap<String, Vec<u8>>,
) -> CommandResult<()> {
    match action.kind {
        CodeFileOperationInverseActionKind::RemovePath => {
            remove_project_path(repo_root, &action.target_path)
        }
        CodeFileOperationInverseActionKind::RenamePath => {
            let source_path = action.source_path.as_deref().ok_or_else(|| {
                CommandError::system_fault(
                    "code_undo_action_invalid",
                    "A rename undo action was missing its source path.",
                )
            })?;
            rename_project_path(repo_root, source_path, &action.target_path)?;
            if let Some(state) = action.restore_state.as_ref() {
                restore_exact_state(repo_root, &action.target_path, state, blob_bytes)?;
            }
            Ok(())
        }
        CodeFileOperationInverseActionKind::RestoreMode
        | CodeFileOperationInverseActionKind::RestorePath
        | CodeFileOperationInverseActionKind::RestoreSymlink => {
            let state = action.restore_state.as_ref().ok_or_else(|| {
                CommandError::system_fault(
                    "code_undo_action_invalid",
                    "A restore undo action was missing the state to restore.",
                )
            })?;
            restore_exact_state(repo_root, &action.target_path, state, blob_bytes)
        }
    }
}

fn rename_project_path(
    repo_root: &Path,
    source_path: &str,
    target_path: &str,
) -> CommandResult<()> {
    let source = absolute_repo_path(repo_root, source_path)?;
    let target = absolute_repo_path(repo_root, target_path)?;
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "code_undo_rename_prepare_failed",
                format!(
                    "Xero could not prepare {} for undo rename: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    fs::rename(&source, &target).map_err(|error| {
        CommandError::retryable(
            "code_undo_rename_failed",
            format!(
                "Xero could not rename {} to {} during undo: {error}",
                source.display(),
                target.display()
            ),
        )
    })
}

fn restore_exact_state(
    repo_root: &Path,
    path: &str,
    state: &CodeExactFileState,
    blob_bytes: &BTreeMap<String, Vec<u8>>,
) -> CommandResult<()> {
    let entry = snapshot_entry_from_exact_state(path, state)?;
    match state.kind {
        CodePatchFileKind::Directory => ensure_directory_entry(repo_root, &entry),
        CodePatchFileKind::File => restore_file_entry(repo_root, &entry, blob_bytes),
        CodePatchFileKind::Symlink => restore_symlink_entry(repo_root, &entry),
    }
}

fn persist_code_undo_history(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    target_commit: &CodePatchsetCommitRecord,
    target_files: &[CodePatchFileRecord],
    parent_head: &CodeWorkspaceHeadRecord,
    result_change_group_id: &str,
    pre_snapshot: &CodeSnapshotRecord,
    post_snapshot: &CodeSnapshotRecord,
) -> CommandResult<PersistedCodeUndo> {
    let before_entries = pre_snapshot.manifest.entry_map();
    let after_entries = post_snapshot.manifest.entry_map();
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    let tx = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "code_undo_change_group_transaction_failed",
            format!("Xero could not start code undo change group transaction: {error}"),
        )
    })?;

    let mut affected_files = Vec::new();
    let mut patch_files = Vec::new();
    for target_file in target_files {
        let (before_entry, after_entry, result_operation, result_target) =
            undo_result_entries(target_file, &before_entries, &after_entries);
        affected_files.push(completed_file_from_entries(
            &result_target,
            result_operation,
            before_entry,
            after_entry,
        ));
        insert_file_version(
            &tx,
            project_id,
            result_change_group_id,
            result_target.path_before.as_deref(),
            result_target.path_after.as_deref(),
            result_operation,
            before_entry,
            after_entry,
            true,
        )?;

        if let Some(file) = patch_file_input_from_entries(
            repo_root,
            project_id,
            patch_files.len(),
            result_operation,
            before_entry,
            after_entry,
        )? {
            patch_files.push(file);
        }
    }

    complete_change_group_tx(
        &tx,
        project_id,
        result_change_group_id,
        &post_snapshot.snapshot_id,
    )?;
    tx.commit().map_err(|error| {
        CommandError::system_fault(
            "code_undo_change_group_commit_failed",
            format!(
                "Xero could not commit code undo change group `{result_change_group_id}` in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    if patch_files.is_empty() {
        return Ok(PersistedCodeUndo {
            history_metadata: None,
            workspace_head: Some(parent_head.clone()),
            affected_files,
        });
    }

    let workspace_epoch = parent_head.workspace_epoch.checked_add(1).ok_or_else(|| {
        CommandError::system_fault(
            "code_workspace_epoch_overflow",
            format!(
                "Xero could not commit undo `{operation_id}` because the workspace epoch is already at the maximum value."
            ),
        )
    })?;
    let commit_id = generate_id("code-commit");
    let tree_id = code_tree_id_for_manifest(&post_snapshot.manifest)?;
    let patchset_id = generate_id("code-patchset");
    let affected_paths = affected_paths_for_patch_files(&patch_files);
    let completed_at = now_timestamp();
    let commit = persist_code_patchset_commit(
        repo_root,
        &CodePatchsetCommitInput {
            project_id: project_id.into(),
            commit_id: commit_id.clone(),
            parent_commit_id: parent_head.head_id.clone(),
            tree_id: tree_id.clone(),
            parent_tree_id: parent_head.tree_id.clone(),
            patchset_id,
            change_group_id: result_change_group_id.into(),
            history_operation_id: Some(operation_id.into()),
            agent_session_id: target_commit.commit.agent_session_id.clone(),
            run_id: target_commit.commit.run_id.clone(),
            tool_call_id: None,
            runtime_event_id: None,
            conversation_sequence: None,
            commit_kind: CodeHistoryCommitKind::Undo,
            summary_label: format!("Undo {}", target_commit.commit.summary_label),
            workspace_epoch,
            created_at: completed_at.clone(),
            completed_at: completed_at.clone(),
            files: patch_files,
        },
    )?;

    let advanced = advance_code_workspace_epoch(
        repo_root,
        &AdvanceCodeWorkspaceEpochRequest {
            project_id: project_id.into(),
            head_id: Some(commit_id.clone()),
            tree_id: Some(tree_id),
            commit_id: Some(commit_id),
            latest_history_operation_id: Some(operation_id.into()),
            affected_paths,
            updated_at: completed_at,
        },
    )?;

    Ok(PersistedCodeUndo {
        history_metadata: Some(code_change_group_history_metadata_from_commit(&commit)),
        workspace_head: Some(advanced.workspace_head),
        affected_files,
    })
}

fn persist_code_session_rollback_history(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    plan: &CodeSessionLineageUndoPlan,
    parent_head: &CodeWorkspaceHeadRecord,
    result_change_group_id: &str,
    pre_snapshot: &CodeSnapshotRecord,
    post_snapshot: &CodeSnapshotRecord,
) -> CommandResult<PersistedCodeUndo> {
    let before_entries = pre_snapshot.manifest.entry_map();
    let after_entries = post_snapshot.manifest.entry_map();
    let changed_targets =
        changed_rollback_targets_from_manifest_diff(&before_entries, &after_entries);
    let patch_files = patch_files_from_manifest_diff(
        repo_root,
        project_id,
        &pre_snapshot.manifest,
        &post_snapshot.manifest,
    )?;
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    let tx = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "code_session_rollback_change_group_transaction_failed",
            format!("Xero could not start code session rollback change group transaction: {error}"),
        )
    })?;

    let mut affected_files = Vec::new();
    for target in changed_targets {
        let before_entry = target
            .path_before
            .as_deref()
            .and_then(|path| before_entries.get(path));
        let after_entry = target
            .path_after
            .as_deref()
            .and_then(|path| after_entries.get(path));
        let operation = infer_file_operation(&target, before_entry, after_entry);
        affected_files.push(completed_file_from_entries(
            &target,
            operation,
            before_entry,
            after_entry,
        ));
        insert_file_version(
            &tx,
            project_id,
            result_change_group_id,
            target.path_before.as_deref(),
            target.path_after.as_deref(),
            operation,
            before_entry,
            after_entry,
            true,
        )?;
    }

    complete_change_group_tx(
        &tx,
        project_id,
        result_change_group_id,
        &post_snapshot.snapshot_id,
    )?;
    tx.commit().map_err(|error| {
        CommandError::system_fault(
            "code_session_rollback_change_group_commit_failed",
            format!(
                "Xero could not commit code session rollback change group `{result_change_group_id}` in {}: {error}",
                database_path.display()
            ),
        )
    })?;

    if patch_files.is_empty() {
        return Ok(PersistedCodeUndo {
            history_metadata: None,
            workspace_head: Some(parent_head.clone()),
            affected_files,
        });
    }

    let workspace_epoch = parent_head.workspace_epoch.checked_add(1).ok_or_else(|| {
        CommandError::system_fault(
            "code_workspace_epoch_overflow",
            format!(
                "Xero could not commit session rollback `{operation_id}` because the workspace epoch is already at the maximum value."
            ),
        )
    })?;
    let commit_id = generate_id("code-commit");
    let tree_id = code_tree_id_for_manifest(&post_snapshot.manifest)?;
    let patchset_id = generate_id("code-patchset");
    let affected_paths = affected_paths_for_patch_files(&patch_files);
    let completed_at = now_timestamp();
    let commit = persist_code_patchset_commit(
        repo_root,
        &CodePatchsetCommitInput {
            project_id: project_id.into(),
            commit_id: commit_id.clone(),
            parent_commit_id: parent_head.head_id.clone(),
            tree_id: tree_id.clone(),
            parent_tree_id: parent_head.tree_id.clone(),
            patchset_id,
            change_group_id: result_change_group_id.into(),
            history_operation_id: Some(operation_id.into()),
            agent_session_id: plan.agent_session_id.clone(),
            run_id: plan.boundary.boundary_run_id.clone(),
            tool_call_id: None,
            runtime_event_id: None,
            conversation_sequence: None,
            commit_kind: CodeHistoryCommitKind::SessionRollback,
            summary_label: session_rollback_summary_label(plan),
            workspace_epoch,
            created_at: completed_at.clone(),
            completed_at: completed_at.clone(),
            files: patch_files,
        },
    )?;

    let advanced = advance_code_workspace_epoch(
        repo_root,
        &AdvanceCodeWorkspaceEpochRequest {
            project_id: project_id.into(),
            head_id: Some(commit_id.clone()),
            tree_id: Some(tree_id),
            commit_id: Some(commit_id),
            latest_history_operation_id: Some(operation_id.into()),
            affected_paths,
            updated_at: completed_at,
        },
    )?;

    Ok(PersistedCodeUndo {
        history_metadata: Some(code_change_group_history_metadata_from_commit(&commit)),
        workspace_head: Some(advanced.workspace_head),
        affected_files,
    })
}

fn changed_rollback_targets_from_manifest_diff(
    before_entries: &BTreeMap<String, CodeSnapshotFileEntry>,
    after_entries: &BTreeMap<String, CodeSnapshotFileEntry>,
) -> Vec<CodeRollbackCaptureTarget> {
    changed_manifest_paths_with_budget(
        before_entries,
        after_entries,
        &BTreeSet::new(),
        CodePatchPayloadBudget::default(),
    )
    .into_iter()
    .filter_map(|path| {
        let before_entry = before_entries.get(&path);
        let after_entry = after_entries.get(&path);
        (before_entry != after_entry).then(|| CodeRollbackCaptureTarget {
            path_before: before_entry.map(|entry| entry.path.clone()),
            path_after: after_entry.map(|entry| entry.path.clone()),
            operation: None,
            explicitly_edited: true,
        })
    })
    .collect()
}

fn changed_manifest_paths_with_budget(
    before_entries: &BTreeMap<String, CodeSnapshotFileEntry>,
    after_entries: &BTreeMap<String, CodeSnapshotFileEntry>,
    explicit_paths: &BTreeSet<String>,
    patch_budget: CodePatchPayloadBudget,
) -> Vec<String> {
    let mut explicit_changed_paths = Vec::new();
    let mut non_explicit_changed_paths = Vec::new();

    for path in before_entries
        .keys()
        .chain(after_entries.keys())
        .cloned()
        .collect::<BTreeSet<_>>()
    {
        let before_entry = before_entries.get(&path);
        let after_entry = after_entries.get(&path);
        if before_entry == after_entry {
            continue;
        }
        if target_overlaps_explicit_paths(before_entry, after_entry, explicit_paths) {
            explicit_changed_paths.push(path);
        } else if non_explicit_changed_paths.len() < patch_budget.max_non_explicit_diff_files {
            non_explicit_changed_paths.push(path);
        }
    }

    explicit_changed_paths.extend(non_explicit_changed_paths);
    explicit_changed_paths
}

fn undo_result_entries<'a>(
    target_file: &CodePatchFileRecord,
    before_entries: &'a BTreeMap<String, CodeSnapshotFileEntry>,
    after_entries: &'a BTreeMap<String, CodeSnapshotFileEntry>,
) -> (
    Option<&'a CodeSnapshotFileEntry>,
    Option<&'a CodeSnapshotFileEntry>,
    CodeFileOperation,
    CodeRollbackCaptureTarget,
) {
    let result_path_before = target_file.path_after.clone();
    let result_path_after = target_file.path_before.clone();
    let before_entry = result_path_before
        .as_deref()
        .and_then(|path| before_entries.get(path));
    let after_entry = result_path_after
        .as_deref()
        .and_then(|path| after_entries.get(path));
    let target = CodeRollbackCaptureTarget {
        path_before: result_path_before,
        path_after: result_path_after,
        operation: None,
        explicitly_edited: true,
    };
    let operation = infer_file_operation(&target, before_entry, after_entry);
    (before_entry, after_entry, operation, target)
}

fn snapshot_entry_from_exact_state(
    path: &str,
    state: &CodeExactFileState,
) -> CommandResult<CodeSnapshotFileEntry> {
    if state.kind == CodePatchFileKind::File && state.blob_id.is_none() {
        return Err(CommandError::retryable(
            "code_rollback_blob_missing",
            format!("Code undo needs a blob id to restore `{path}`."),
        ));
    }
    Ok(CodeSnapshotFileEntry {
        path: path.into(),
        kind: snapshot_file_kind_from_patch(state.kind),
        size: state.size,
        sha256: state.content_hash.clone(),
        blob_id: state.blob_id.clone(),
        mode: state.mode,
        modified_at_millis: None,
        symlink_target: state.symlink_target.clone(),
    })
}

fn exact_state_from_snapshot_entry(entry: &CodeSnapshotFileEntry) -> CodeExactFileState {
    CodeExactFileState {
        kind: patch_file_kind_from_snapshot(entry),
        content_hash: entry.sha256.clone(),
        blob_id: entry.blob_id.clone(),
        size: entry.size,
        mode: entry.mode,
        symlink_target: entry.symlink_target.clone(),
    }
}

fn snapshot_file_kind_from_patch(kind: CodePatchFileKind) -> CodeSnapshotFileKind {
    match kind {
        CodePatchFileKind::File => CodeSnapshotFileKind::File,
        CodePatchFileKind::Directory => CodeSnapshotFileKind::Directory,
        CodePatchFileKind::Symlink => CodeSnapshotFileKind::Symlink,
    }
}

fn code_file_undo_conflict_from_text(
    conflict: &CodeTextInversePatchConflict,
) -> CodeFileUndoConflict {
    CodeFileUndoConflict {
        path: conflict.path.clone(),
        kind: match conflict.kind {
            CodeTextInversePatchConflictKind::TextOverlap => CodeFileUndoConflictKind::TextOverlap,
            CodeTextInversePatchConflictKind::FileMissing => CodeFileUndoConflictKind::FileMissing,
            CodeTextInversePatchConflictKind::UnsupportedOperation => {
                CodeFileUndoConflictKind::UnsupportedOperation
            }
        },
        message: conflict.message.clone(),
        base_hash: conflict.base_hash.clone(),
        selected_hash: conflict.selected_hash.clone(),
        current_hash: conflict.current_hash.clone(),
        hunk_ids: conflict.hunk_ids.clone(),
    }
}

fn code_file_undo_conflict_from_file_operation(
    conflict: &CodeFileOperationInverseConflict,
) -> CodeFileUndoConflict {
    CodeFileUndoConflict {
        path: conflict.path.clone(),
        kind: match conflict.kind {
            CodeFileOperationInverseConflictKind::CurrentStateMismatch => {
                if metadata_conflict(&conflict.expected_state, &conflict.current_state) {
                    CodeFileUndoConflictKind::MetadataMismatch
                } else {
                    CodeFileUndoConflictKind::ContentMismatch
                }
            }
            CodeFileOperationInverseConflictKind::PathAlreadyExists => {
                CodeFileUndoConflictKind::FileExists
            }
            CodeFileOperationInverseConflictKind::PathMissing => {
                CodeFileUndoConflictKind::FileMissing
            }
            CodeFileOperationInverseConflictKind::UnsupportedOperation => {
                CodeFileUndoConflictKind::UnsupportedOperation
            }
        },
        message: conflict.message.clone(),
        base_hash: conflict
            .expected_state
            .as_ref()
            .and_then(|state| state.content_hash.clone()),
        selected_hash: None,
        current_hash: conflict
            .current_state
            .as_ref()
            .and_then(|state| state.content_hash.clone()),
        hunk_ids: Vec::new(),
    }
}

fn code_file_undo_conflict_from_session_lineage(
    conflict: &CodeSessionLineageUndoPlanConflict,
) -> CodeFileUndoConflict {
    CodeFileUndoConflict {
        path: conflict.path.clone(),
        kind: match conflict.kind {
            CodeSessionLineageUndoPlanConflictKind::TextOverlap => {
                CodeFileUndoConflictKind::TextOverlap
            }
            CodeSessionLineageUndoPlanConflictKind::FileMissing
            | CodeSessionLineageUndoPlanConflictKind::PathMissing => {
                CodeFileUndoConflictKind::FileMissing
            }
            CodeSessionLineageUndoPlanConflictKind::PathAlreadyExists => {
                CodeFileUndoConflictKind::FileExists
            }
            CodeSessionLineageUndoPlanConflictKind::CurrentStateMismatch => {
                CodeFileUndoConflictKind::ContentMismatch
            }
            CodeSessionLineageUndoPlanConflictKind::UnsupportedOperation => {
                CodeFileUndoConflictKind::UnsupportedOperation
            }
        },
        message: conflict.message.clone(),
        base_hash: conflict.base_hash.clone(),
        selected_hash: conflict.selected_hash.clone(),
        current_hash: conflict.current_hash.clone(),
        hunk_ids: conflict.hunk_ids.clone(),
    }
}

fn metadata_conflict(
    expected: &Option<CodeExactFileState>,
    current: &Option<CodeExactFileState>,
) -> bool {
    match (expected, current) {
        (Some(expected), Some(current)) => {
            expected.content_hash == current.content_hash
                && (expected.mode != current.mode
                    || expected.kind != current.kind
                    || expected.symlink_target != current.symlink_target)
        }
        _ => false,
    }
}

fn conflicted_code_session_rollback_result(
    request: &ApplyCodeSessionRollbackRequest,
    operation_id: &str,
    plan: &CodeSessionLineageUndoPlan,
    affected_paths: Vec<String>,
    conflicts: Vec<CodeFileUndoConflict>,
    workspace_head: Option<CodeWorkspaceHeadRecord>,
    patch_availability: Option<CodeChangeGroupHistoryMetadataRecord>,
) -> AppliedCodeSessionRollback {
    AppliedCodeSessionRollback {
        project_id: request.boundary.project_id.clone(),
        agent_session_id: request.boundary.agent_session_id.clone(),
        run_id: request
            .boundary
            .run_id
            .clone()
            .unwrap_or_else(|| plan.boundary.boundary_run_id.clone()),
        operation_id: operation_id.into(),
        status: CodeFileUndoApplyStatus::Conflicted,
        target_id: request.boundary.target_id.clone(),
        boundary_id: request.boundary.boundary_id.clone(),
        boundary_change_group_id: plan.boundary.boundary_change_group_id.clone(),
        target_change_group_ids: session_rollback_target_change_group_ids(plan),
        result_change_group_id: None,
        result_commit_id: None,
        affected_paths,
        conflicts,
        workspace_head,
        patch_availability,
        affected_files: Vec::new(),
    }
}

fn conflicted_code_file_undo_result(
    request: &ApplyCodeFileUndoRequest,
    operation_id: &str,
    agent_session_id: &str,
    run_id: &str,
    target_file: &CodePatchFileRecord,
    affected_paths: Vec<String>,
    conflicts: Vec<CodeFileUndoConflict>,
    workspace_head: Option<CodeWorkspaceHeadRecord>,
    patch_availability: Option<CodeChangeGroupHistoryMetadataRecord>,
) -> AppliedCodeFileUndo {
    AppliedCodeFileUndo {
        project_id: request.project_id.clone(),
        agent_session_id: agent_session_id.into(),
        run_id: run_id.into(),
        operation_id: operation_id.into(),
        status: CodeFileUndoApplyStatus::Conflicted,
        target_change_group_id: request.target_change_group_id.clone(),
        target_patch_file_id: target_file.patch_file_id.clone(),
        target_file_path: patch_file_result_path(target_file),
        selected_hunk_ids: request.target_hunk_ids.clone(),
        result_change_group_id: None,
        result_commit_id: None,
        affected_paths,
        conflicts,
        workspace_head,
        patch_availability,
        affected_files: Vec::new(),
    }
}

fn conflicted_code_change_group_undo_result(
    request: &ApplyCodeChangeGroupUndoRequest,
    operation_id: &str,
    agent_session_id: &str,
    run_id: &str,
    affected_paths: Vec<String>,
    conflicts: Vec<CodeFileUndoConflict>,
    workspace_head: Option<CodeWorkspaceHeadRecord>,
    patch_availability: Option<CodeChangeGroupHistoryMetadataRecord>,
) -> AppliedCodeChangeGroupUndo {
    AppliedCodeChangeGroupUndo {
        project_id: request.project_id.clone(),
        agent_session_id: agent_session_id.into(),
        run_id: run_id.into(),
        operation_id: operation_id.into(),
        status: CodeFileUndoApplyStatus::Conflicted,
        target_change_group_id: request.target_change_group_id.clone(),
        result_change_group_id: None,
        result_commit_id: None,
        affected_paths,
        conflicts,
        workspace_head,
        patch_availability,
        affected_files: Vec::new(),
    }
}

fn session_rollback_target_change_group_ids(plan: &CodeSessionLineageUndoPlan) -> Vec<String> {
    plan.target_change_groups
        .iter()
        .map(|group| group.change_group_id.clone())
        .collect()
}

fn session_rollback_summary_label(plan: &CodeSessionLineageUndoPlan) -> String {
    format!("Return session to {}", plan.boundary.commit.summary_label)
}

fn affected_paths_for_one_patch_file(file: &CodePatchFileRecord) -> Vec<String> {
    let mut paths = BTreeSet::new();
    if let Some(path) = file.path_before.as_deref() {
        paths.insert(path.to_string());
    }
    if let Some(path) = file.path_after.as_deref() {
        paths.insert(path.to_string());
    }
    paths.into_iter().collect()
}

fn affected_paths_for_patch_records(files: &[CodePatchFileRecord]) -> Vec<String> {
    files
        .iter()
        .flat_map(affected_paths_for_one_patch_file)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn patch_file_result_path(file: &CodePatchFileRecord) -> String {
    file.path_after
        .as_deref()
        .or(file.path_before.as_deref())
        .unwrap_or("<unknown>")
        .to_string()
}

fn file_undo_history_target_kind(
    request: &ApplyCodeFileUndoRequest,
) -> CodeHistoryOperationTargetKind {
    if request.target_hunk_ids.is_empty() {
        CodeHistoryOperationTargetKind::FileChange
    } else {
        CodeHistoryOperationTargetKind::Hunks
    }
}

fn file_undo_history_target_id(
    request: &ApplyCodeFileUndoRequest,
    target_file: &CodePatchFileRecord,
) -> String {
    if request.target_hunk_ids.is_empty() {
        request
            .target_patch_file_id
            .clone()
            .unwrap_or_else(|| target_file.patch_file_id.clone())
    } else {
        format!(
            "{}:{}",
            target_file.patch_file_id,
            request.target_hunk_ids.join(",")
        )
    }
}

fn session_rollback_history_target_kind(
    kind: CodeSessionBoundaryTargetKind,
) -> CodeHistoryOperationTargetKind {
    match kind {
        CodeSessionBoundaryTargetKind::SessionBoundary => {
            CodeHistoryOperationTargetKind::SessionBoundary
        }
        CodeSessionBoundaryTargetKind::RunBoundary => CodeHistoryOperationTargetKind::RunBoundary,
    }
}

pub fn maintain_code_rollback_storage(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<CodeRollbackMaintenanceReport> {
    let diagnostics = validate_code_rollback_storage(repo_root, project_id)?;
    let prune = prune_code_rollback_blobs(
        repo_root,
        project_id,
        CodeRollbackRetentionPolicy::default(),
    )?;
    let report = CodeRollbackMaintenanceReport {
        project_id: project_id.into(),
        generated_at: now_timestamp(),
        diagnostics,
        prune,
    };
    write_code_rollback_maintenance_report(repo_root, &report)?;
    Ok(report)
}

pub fn prune_code_rollback_blobs(
    repo_root: &Path,
    project_id: &str,
    policy: CodeRollbackRetentionPolicy,
) -> CommandResult<CodeRollbackPruneReport> {
    validate_non_empty(project_id, "projectId")?;
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let reachable_blob_ids =
        collect_reachable_code_blob_ids(&connection, &database_path, project_id)?;
    let blob_rows = collect_code_blob_rows(&connection, project_id)?;
    let mut report = CodeRollbackPruneReport {
        project_id: project_id.into(),
        scanned_blob_count: blob_rows.len(),
        reachable_blob_count: reachable_blob_ids.len(),
        pruned_blob_count: 0,
        retained_unreferenced_blob_count: 0,
        missing_blob_file_count: 0,
        pruned_bytes: 0,
        diagnostics: Vec::new(),
    };

    for blob in blob_rows {
        if reachable_blob_ids.contains(&blob.blob_id) {
            continue;
        }

        match blob_is_old_enough(&blob.created_at, policy.min_unreferenced_age_seconds) {
            Ok(true) => {}
            Ok(false) => {
                report.retained_unreferenced_blob_count += 1;
                continue;
            }
            Err(error) => {
                report.retained_unreferenced_blob_count += 1;
                report.diagnostics.push(CodeRollbackStorageDiagnostic {
                    code: error.code,
                    snapshot_id: None,
                    message: error.message,
                });
                continue;
            }
        }

        match prune_unreferenced_blob(&connection, repo_root, project_id, &blob) {
            Ok(PrunedBlobOutcome::Pruned) => {
                report.pruned_blob_count += 1;
                report.pruned_bytes = report.pruned_bytes.saturating_add(blob.size_bytes);
            }
            Ok(PrunedBlobOutcome::MetadataPrunedMissingFile) => {
                report.pruned_blob_count += 1;
                report.missing_blob_file_count += 1;
                report.pruned_bytes = report.pruned_bytes.saturating_add(blob.size_bytes);
                report.diagnostics.push(CodeRollbackStorageDiagnostic {
                    code: "code_blob_file_missing_pruned".into(),
                    snapshot_id: None,
                    message: format!(
                        "Unreferenced code blob `{}` had metadata but no blob file; metadata was pruned.",
                        blob.blob_id
                    ),
                });
            }
            Err(error) => {
                report.retained_unreferenced_blob_count += 1;
                report.diagnostics.push(CodeRollbackStorageDiagnostic {
                    code: error.code,
                    snapshot_id: None,
                    message: error.message,
                });
            }
        }
    }

    Ok(report)
}

pub fn validate_code_rollback_storage(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<Vec<CodeRollbackStorageDiagnostic>> {
    validate_non_empty(project_id, "projectId")?;
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;

    let mut diagnostics = Vec::new();
    let pending_snapshot_ids = collect_snapshot_ids_by_state(&connection, project_id, "pending")?;
    for snapshot_id in pending_snapshot_ids {
        let diagnostic = json!({
            "code": "code_snapshot_write_incomplete",
            "message": "Snapshot write was still pending at startup and has been marked failed.",
        });
        mark_snapshot_failed(&connection, project_id, &snapshot_id, diagnostic)?;
        diagnostics.push(CodeRollbackStorageDiagnostic {
            code: "code_snapshot_write_incomplete".into(),
            snapshot_id: Some(snapshot_id),
            message: "Snapshot write was still pending at startup and has been marked failed."
                .into(),
        });
    }

    let completed = collect_completed_snapshot_rows(&connection, project_id)?;
    for (snapshot_id, manifest_json) in completed {
        let manifest = match serde_json::from_str::<CodeSnapshotManifest>(&manifest_json) {
            Ok(manifest) => manifest,
            Err(error) => {
                let diagnostic = json!({
                    "code": "code_snapshot_manifest_invalid",
                    "message": format!("Snapshot manifest could not be decoded: {error}"),
                });
                mark_snapshot_failed(&connection, project_id, &snapshot_id, diagnostic)?;
                diagnostics.push(CodeRollbackStorageDiagnostic {
                    code: "code_snapshot_manifest_invalid".into(),
                    snapshot_id: Some(snapshot_id),
                    message: format!("Snapshot manifest could not be decoded: {error}"),
                });
                continue;
            }
        };
        if snapshot_manifest_root_moved(repo_root, &manifest) {
            diagnostics.push(CodeRollbackStorageDiagnostic {
                code: "code_snapshot_root_moved".into(),
                snapshot_id: Some(snapshot_id.clone()),
                message: format!(
                    "Snapshot `{snapshot_id}` was captured for `{}` but the project now resolves to `{}`.",
                    manifest.root_path,
                    repo_root.display()
                ),
            });
        }
        if let Err(error) = load_required_blob_bytes(&connection, repo_root, project_id, &manifest)
        {
            let diagnostic = json!({
                "code": error.code,
                "message": error.message,
            });
            mark_snapshot_failed(&connection, project_id, &snapshot_id, diagnostic)?;
            diagnostics.push(CodeRollbackStorageDiagnostic {
                code: error.code,
                snapshot_id: Some(snapshot_id),
                message: error.message,
            });
        }
    }

    diagnostics.extend(recover_pending_rollback_operations(
        repo_root,
        &connection,
        project_id,
    )?);
    diagnostics.extend(recover_interrupted_code_history_operations(
        repo_root,
        &connection,
        project_id,
    )?);

    Ok(diagnostics)
}

pub fn list_code_rollback_operations_for_session(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
) -> CommandResult<Vec<CodeRollbackOperationRecord>> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(agent_session_id, "agentSessionId")?;
    if let Some(run_id) = run_id {
        validate_non_empty(run_id, "runId")?;
    }

    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    if let Some(run_id) = run_id {
        query_code_rollback_operations_for_run(
            &connection,
            &database_path,
            project_id,
            agent_session_id,
            run_id,
        )
    } else {
        query_code_rollback_operations_for_session(
            &connection,
            &database_path,
            project_id,
            agent_session_id,
        )
    }
}

pub fn list_code_history_operations_for_session(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: Option<&str>,
) -> CommandResult<Vec<CodeHistoryOperationRecord>> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(agent_session_id, "agentSessionId")?;
    if let Some(run_id) = run_id {
        validate_non_empty(run_id, "runId")?;
    }

    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    if let Some(run_id) = run_id {
        query_code_history_operations_for_run(
            &connection,
            &database_path,
            project_id,
            agent_session_id,
            run_id,
        )
    } else {
        query_code_history_operations_for_session(
            &connection,
            &database_path,
            project_id,
            agent_session_id,
        )
    }
}

pub fn list_recent_code_rollback_operations_for_session(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    limit: usize,
) -> CommandResult<Vec<CodeRollbackOperationRecord>> {
    validate_non_empty(project_id, "projectId")?;
    validate_non_empty(agent_session_id, "agentSessionId")?;
    let limit = limit.clamp(1, 25);

    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT *
            FROM (
                SELECT
                    operation.project_id,
                    operation.operation_id,
                    operation.agent_session_id,
                    operation.run_id,
                    operation.target_change_group_id,
                    operation.target_snapshot_id,
                    operation.pre_rollback_snapshot_id,
                    operation.result_change_group_id,
                    operation.status,
                    operation.failure_code,
                    operation.failure_message,
                    operation.affected_files_json,
                    target.summary_label AS target_summary_label,
                    result.summary_label AS result_summary_label,
                    operation.created_at,
                    operation.completed_at
                FROM code_rollback_operations operation
                LEFT JOIN code_change_groups target
                  ON target.project_id = operation.project_id
                 AND target.change_group_id = operation.target_change_group_id
                LEFT JOIN code_change_groups result
                  ON result.project_id = operation.project_id
                 AND result.change_group_id = operation.result_change_group_id
                WHERE operation.project_id = ?1
                  AND operation.agent_session_id = ?2
                ORDER BY operation.created_at DESC, operation.operation_id DESC
                LIMIT ?3
            )
            ORDER BY created_at ASC, operation_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!(
                    "Xero could not prepare recent code rollback operation query in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let mapped = statement
        .query_map(params![project_id, agent_session_id, limit as i64], |row| {
            read_code_rollback_operation_row(row)
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!(
                    "Xero could not query recent code rollback operations in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    collect_code_rollback_operation_rows(mapped, &database_path)
}

struct SnapshotCaptureRequest {
    project_id: String,
    agent_session_id: String,
    run_id: String,
    change_group_id: Option<String>,
    boundary_kind: CodeSnapshotBoundaryKind,
    previous_snapshot_id: Option<String>,
    explicit_paths: BTreeSet<String>,
}

fn capture_code_snapshot_internal(
    repo_root: &Path,
    request: SnapshotCaptureRequest,
) -> CommandResult<CodeSnapshotRecord> {
    capture_code_snapshot_internal_with_budget(
        repo_root,
        request,
        CodeSnapshotScanBudget::default(),
    )
}

fn capture_code_snapshot_internal_with_budget(
    repo_root: &Path,
    request: SnapshotCaptureRequest,
    scan_budget: CodeSnapshotScanBudget,
) -> CommandResult<CodeSnapshotRecord> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.agent_session_id, "agentSessionId")?;
    validate_non_empty(&request.run_id, "runId")?;
    if let Some(change_group_id) = request.change_group_id.as_deref() {
        validate_non_empty(change_group_id, "changeGroupId")?;
    }

    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, &request.project_id)?;

    let snapshot_id = generate_id("code-snapshot");
    insert_pending_snapshot(&connection, repo_root, &request, &snapshot_id)?;

    let previous_manifest = match request.previous_snapshot_id.as_deref() {
        Some(previous_snapshot_id) => Some(load_completed_snapshot_manifest(
            &connection,
            &database_path,
            &request.project_id,
            previous_snapshot_id,
        )?),
        None => None,
    };

    let scan_result = scan_project_manifest(
        repo_root,
        &connection,
        &request.project_id,
        previous_manifest.as_ref(),
        &request.explicit_paths,
        scan_budget,
    );

    let manifest = match scan_result {
        Ok(manifest) => manifest,
        Err(error) => {
            let diagnostic = json!({
                "code": error.code,
                "message": error.message,
            });
            let _ =
                mark_snapshot_failed(&connection, &request.project_id, &snapshot_id, diagnostic);
            return Err(error);
        }
    };

    complete_snapshot(&connection, &request.project_id, &snapshot_id, &manifest)?;
    let completed_at = now_timestamp();
    Ok(CodeSnapshotRecord {
        project_id: request.project_id,
        agent_session_id: request.agent_session_id,
        run_id: request.run_id,
        snapshot_id,
        change_group_id: request.change_group_id,
        boundary_kind: request.boundary_kind,
        manifest,
        created_at: completed_at.clone(),
        completed_at,
    })
}

fn insert_change_group_open(
    repo_root: &Path,
    input: &CodeChangeGroupInput,
    change_group_id: &str,
) -> CommandResult<()> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, &input.project_id)?;
    let started_at = now_timestamp();
    connection
        .execute(
            r#"
            INSERT INTO code_change_groups (
                project_id,
                agent_session_id,
                run_id,
                change_group_id,
                parent_change_group_id,
                tool_call_id,
                runtime_event_id,
                conversation_sequence,
                change_kind,
                summary_label,
                restore_state,
                status,
                started_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, 'open', ?12)
            "#,
            params![
                input.project_id,
                input.agent_session_id,
                input.run_id,
                change_group_id,
                input.parent_change_group_id,
                input.tool_call_id,
                input.runtime_event_id,
                input.conversation_sequence,
                input.change_kind.as_str(),
                input.summary_label,
                input.restore_state.as_str(),
                started_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_change_group_insert_failed",
                format!(
                    "Xero could not persist code change group `{change_group_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

fn update_change_group_before_snapshot(
    repo_root: &Path,
    project_id: &str,
    change_group_id: &str,
    before_snapshot_id: &str,
) -> CommandResult<()> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE code_change_groups
            SET before_snapshot_id = ?3
            WHERE project_id = ?1
              AND change_group_id = ?2
            "#,
            params![project_id, change_group_id, before_snapshot_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_change_group_update_failed",
                format!(
                    "Xero could not attach snapshot `{before_snapshot_id}` to code change group `{change_group_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

fn persist_exact_file_versions(
    repo_root: &Path,
    handle: &CodeRollbackCaptureHandle,
    after_snapshot_id: &str,
) -> CommandResult<CompletedCodeChangeGroup> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    let before_manifest = load_completed_snapshot_manifest(
        &connection,
        &database_path,
        &handle.project_id,
        &handle.before_snapshot_id,
    )?;
    let after_manifest = load_completed_snapshot_manifest(
        &connection,
        &database_path,
        &handle.project_id,
        after_snapshot_id,
    )?;
    let before_entries = before_manifest.entry_map();
    let after_entries = after_manifest.entry_map();
    let tx = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "code_change_group_transaction_failed",
            format!("Xero could not start code change group transaction: {error}"),
        )
    })?;

    let mut affected_files = Vec::new();
    for target in &handle.targets {
        let before_entry = target
            .path_before
            .as_deref()
            .and_then(|path| before_entries.get(path));
        let after_entry = target
            .path_after
            .as_deref()
            .and_then(|path| after_entries.get(path));
        let operation = target
            .operation
            .unwrap_or_else(|| infer_file_operation(target, before_entry, after_entry));

        if before_entry == after_entry
            && target.path_before == target.path_after
            && !matches!(
                operation,
                CodeFileOperation::Create | CodeFileOperation::Delete | CodeFileOperation::Rename
            )
        {
            continue;
        }

        insert_file_version(
            &tx,
            &handle.project_id,
            &handle.change_group_id,
            target.path_before.as_deref(),
            target.path_after.as_deref(),
            operation,
            before_entry,
            after_entry,
            target.explicitly_edited,
        )?;
        affected_files.push(completed_file_from_entries(
            target,
            operation,
            before_entry,
            after_entry,
        ));
    }

    complete_change_group_tx(
        &tx,
        &handle.project_id,
        &handle.change_group_id,
        after_snapshot_id,
    )?;
    tx.commit().map_err(|error| {
        CommandError::system_fault(
            "code_change_group_commit_failed",
            format!(
                "Xero could not commit code change group `{}` in {}: {error}",
                handle.change_group_id,
                database_path.display()
            ),
        )
    })?;

    let history_metadata =
        persist_exact_capture_history_commit(repo_root, handle, &before_manifest, &after_manifest)?;
    let completed = CompletedCodeChangeGroup {
        project_id: handle.project_id.clone(),
        agent_session_id: handle.agent_session_id.clone(),
        run_id: handle.run_id.clone(),
        change_group_id: handle.change_group_id.clone(),
        before_snapshot_id: handle.before_snapshot_id.clone(),
        after_snapshot_id: after_snapshot_id.into(),
        file_version_count: affected_files.len(),
        affected_files,
        history_metadata,
    };

    Ok(completed)
}

fn persist_broad_file_versions(
    repo_root: &Path,
    handle: &CodeRollbackCaptureHandle,
    after_snapshot_id: &str,
    explicit_paths: &BTreeSet<String>,
    patch_budget: CodePatchPayloadBudget,
) -> CommandResult<CompletedCodeChangeGroup> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    let before_manifest = load_completed_snapshot_manifest(
        &connection,
        &database_path,
        &handle.project_id,
        &handle.before_snapshot_id,
    )?;
    let after_manifest = load_completed_snapshot_manifest(
        &connection,
        &database_path,
        &handle.project_id,
        after_snapshot_id,
    )?;
    let before_entries = before_manifest.entry_map();
    let after_entries = after_manifest.entry_map();
    let changed_paths = changed_manifest_paths_with_budget(
        &before_entries,
        &after_entries,
        explicit_paths,
        patch_budget,
    );

    let tx = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "code_change_group_transaction_failed",
            format!("Xero could not start code change group transaction: {error}"),
        )
    })?;

    let mut affected_files = Vec::new();
    for path in changed_paths {
        let before_entry = before_entries.get(&path);
        let after_entry = after_entries.get(&path);
        let target = CodeRollbackCaptureTarget {
            path_before: before_entry.map(|entry| entry.path.clone()),
            path_after: after_entry.map(|entry| entry.path.clone()),
            operation: None,
            explicitly_edited: target_overlaps_explicit_paths(
                before_entry,
                after_entry,
                explicit_paths,
            ),
        };
        let operation = infer_file_operation(&target, before_entry, after_entry);
        insert_file_version(
            &tx,
            &handle.project_id,
            &handle.change_group_id,
            target.path_before.as_deref(),
            target.path_after.as_deref(),
            operation,
            before_entry,
            after_entry,
            target.explicitly_edited,
        )?;
        affected_files.push(completed_file_from_entries(
            &target,
            operation,
            before_entry,
            after_entry,
        ));
    }

    complete_change_group_tx(
        &tx,
        &handle.project_id,
        &handle.change_group_id,
        after_snapshot_id,
    )?;
    tx.commit().map_err(|error| {
        CommandError::system_fault(
            "code_change_group_commit_failed",
            format!(
                "Xero could not commit code change group `{}` in {}: {error}",
                handle.change_group_id,
                database_path.display()
            ),
        )
    })?;

    let history_metadata = persist_broad_capture_history_commit(
        repo_root,
        handle,
        &before_manifest,
        &after_manifest,
        explicit_paths,
        patch_budget,
    )?;
    let completed = CompletedCodeChangeGroup {
        project_id: handle.project_id.clone(),
        agent_session_id: handle.agent_session_id.clone(),
        run_id: handle.run_id.clone(),
        change_group_id: handle.change_group_id.clone(),
        before_snapshot_id: handle.before_snapshot_id.clone(),
        after_snapshot_id: after_snapshot_id.into(),
        file_version_count: affected_files.len(),
        affected_files,
        history_metadata,
    };

    Ok(completed)
}

#[derive(Debug)]
struct CompletedChangeGroupCommitMetadata {
    tool_call_id: Option<String>,
    runtime_event_id: Option<i64>,
    conversation_sequence: Option<i64>,
    change_kind: String,
    summary_label: String,
    started_at: String,
    completed_at: String,
}

fn persist_exact_capture_history_commit(
    repo_root: &Path,
    handle: &CodeRollbackCaptureHandle,
    before_manifest: &CodeSnapshotManifest,
    after_manifest: &CodeSnapshotManifest,
) -> CommandResult<Option<CodeChangeGroupHistoryMetadataRecord>> {
    let before_entries = before_manifest.entry_map();
    let after_entries = after_manifest.entry_map();
    let mut files = Vec::new();

    for target in &handle.targets {
        let before_entry = target
            .path_before
            .as_deref()
            .and_then(|path| before_entries.get(path));
        let after_entry = target
            .path_after
            .as_deref()
            .and_then(|path| after_entries.get(path));
        let requested_operation = target
            .operation
            .unwrap_or_else(|| infer_file_operation(target, before_entry, after_entry));
        if before_entry == after_entry && target.path_before == target.path_after {
            continue;
        }
        let Some(file) = patch_file_input_from_entries(
            repo_root,
            &handle.project_id,
            files.len(),
            requested_operation,
            before_entry,
            after_entry,
        )?
        else {
            continue;
        };
        files.push(file);
    }

    if files.is_empty() {
        return Ok(None);
    }

    let metadata = load_completed_change_group_commit_metadata(
        repo_root,
        &handle.project_id,
        &handle.change_group_id,
    )?;
    let parent_head = ensure_code_workspace_head(repo_root, &handle.project_id)?;
    let workspace_epoch = parent_head.workspace_epoch.checked_add(1).ok_or_else(|| {
        CommandError::system_fault(
            "code_workspace_epoch_overflow",
            format!(
                "Xero could not commit exact-path capture `{}` because the workspace epoch is already at the maximum value.",
                handle.change_group_id
            ),
        )
    })?;
    let commit_id = generate_id("code-commit");
    let tree_id = code_tree_id_for_manifest(after_manifest)?;
    let patchset_id = generate_id("code-patchset");
    let affected_paths = affected_paths_for_patch_files(&files);
    let updated_at = metadata.completed_at.clone();

    let commit = persist_code_patchset_commit(
        repo_root,
        &CodePatchsetCommitInput {
            project_id: handle.project_id.clone(),
            commit_id: commit_id.clone(),
            parent_commit_id: parent_head.head_id.clone(),
            tree_id: tree_id.clone(),
            parent_tree_id: parent_head.tree_id.clone(),
            patchset_id,
            change_group_id: handle.change_group_id.clone(),
            history_operation_id: None,
            agent_session_id: handle.agent_session_id.clone(),
            run_id: handle.run_id.clone(),
            tool_call_id: metadata.tool_call_id,
            runtime_event_id: metadata.runtime_event_id,
            conversation_sequence: metadata.conversation_sequence,
            commit_kind: commit_kind_for_change_kind(&metadata.change_kind),
            summary_label: metadata.summary_label,
            workspace_epoch,
            created_at: metadata.started_at,
            completed_at: updated_at.clone(),
            files,
        },
    )?;

    advance_code_workspace_epoch(
        repo_root,
        &AdvanceCodeWorkspaceEpochRequest {
            project_id: handle.project_id.clone(),
            head_id: Some(commit_id.clone()),
            tree_id: Some(tree_id),
            commit_id: Some(commit_id),
            latest_history_operation_id: None,
            affected_paths,
            updated_at,
        },
    )?;

    Ok(Some(code_change_group_history_metadata_from_commit(
        &commit,
    )))
}

fn persist_broad_capture_history_commit(
    repo_root: &Path,
    handle: &CodeRollbackCaptureHandle,
    before_manifest: &CodeSnapshotManifest,
    after_manifest: &CodeSnapshotManifest,
    explicit_paths: &BTreeSet<String>,
    patch_budget: CodePatchPayloadBudget,
) -> CommandResult<Option<CodeChangeGroupHistoryMetadataRecord>> {
    let files = patch_files_from_manifest_diff_with_budget(
        repo_root,
        &handle.project_id,
        before_manifest,
        after_manifest,
        explicit_paths,
        patch_budget,
    )?;
    if files.is_empty() {
        return Ok(None);
    }

    let metadata = load_completed_change_group_commit_metadata(
        repo_root,
        &handle.project_id,
        &handle.change_group_id,
    )?;
    let parent_head = ensure_code_workspace_head(repo_root, &handle.project_id)?;
    let workspace_epoch = parent_head.workspace_epoch.checked_add(1).ok_or_else(|| {
        CommandError::system_fault(
            "code_workspace_epoch_overflow",
            format!(
                "Xero could not commit broad capture `{}` because the workspace epoch is already at the maximum value.",
                handle.change_group_id
            ),
        )
    })?;
    let commit_id = generate_id("code-commit");
    let tree_id = code_tree_id_for_manifest(after_manifest)?;
    let patchset_id = generate_id("code-patchset");
    let affected_paths = affected_paths_for_patch_files(&files);
    let updated_at = metadata.completed_at.clone();

    let commit = persist_code_patchset_commit(
        repo_root,
        &CodePatchsetCommitInput {
            project_id: handle.project_id.clone(),
            commit_id: commit_id.clone(),
            parent_commit_id: parent_head.head_id.clone(),
            tree_id: tree_id.clone(),
            parent_tree_id: parent_head.tree_id.clone(),
            patchset_id,
            change_group_id: handle.change_group_id.clone(),
            history_operation_id: None,
            agent_session_id: handle.agent_session_id.clone(),
            run_id: handle.run_id.clone(),
            tool_call_id: metadata.tool_call_id,
            runtime_event_id: metadata.runtime_event_id,
            conversation_sequence: metadata.conversation_sequence,
            commit_kind: commit_kind_for_change_kind(&metadata.change_kind),
            summary_label: metadata.summary_label,
            workspace_epoch,
            created_at: metadata.started_at,
            completed_at: updated_at.clone(),
            files,
        },
    )?;

    advance_code_workspace_epoch(
        repo_root,
        &AdvanceCodeWorkspaceEpochRequest {
            project_id: handle.project_id.clone(),
            head_id: Some(commit_id.clone()),
            tree_id: Some(tree_id),
            commit_id: Some(commit_id),
            latest_history_operation_id: None,
            affected_paths,
            updated_at,
        },
    )?;

    Ok(Some(code_change_group_history_metadata_from_commit(
        &commit,
    )))
}

fn patch_files_from_manifest_diff(
    repo_root: &Path,
    project_id: &str,
    before_manifest: &CodeSnapshotManifest,
    after_manifest: &CodeSnapshotManifest,
) -> CommandResult<Vec<CodePatchFileInput>> {
    patch_files_from_manifest_diff_with_budget(
        repo_root,
        project_id,
        before_manifest,
        after_manifest,
        &BTreeSet::new(),
        CodePatchPayloadBudget::default(),
    )
}

fn patch_files_from_manifest_diff_with_budget(
    repo_root: &Path,
    project_id: &str,
    before_manifest: &CodeSnapshotManifest,
    after_manifest: &CodeSnapshotManifest,
    explicit_paths: &BTreeSet<String>,
    patch_budget: CodePatchPayloadBudget,
) -> CommandResult<Vec<CodePatchFileInput>> {
    let before_entries = before_manifest.entry_map();
    let after_entries = after_manifest.entry_map();
    let changed_paths = changed_manifest_paths_with_budget(
        &before_entries,
        &after_entries,
        explicit_paths,
        patch_budget,
    );
    let mut files = Vec::new();

    for path in changed_paths {
        let before_entry = before_entries.get(&path);
        let after_entry = after_entries.get(&path);
        let target = CodeRollbackCaptureTarget {
            path_before: before_entry.map(|entry| entry.path.clone()),
            path_after: after_entry.map(|entry| entry.path.clone()),
            operation: None,
            explicitly_edited: false,
        };
        let operation = infer_file_operation(&target, before_entry, after_entry);
        let Some(file) = patch_file_input_from_entries_with_hunk_budget(
            repo_root,
            project_id,
            files.len(),
            operation,
            before_entry,
            after_entry,
            patch_budget.max_text_hunk_payload_bytes,
        )?
        else {
            continue;
        };
        files.push(file);
    }

    Ok(files)
}

fn patch_file_input_from_entries(
    repo_root: &Path,
    project_id: &str,
    file_index: usize,
    requested_operation: CodeFileOperation,
    before_entry: Option<&CodeSnapshotFileEntry>,
    after_entry: Option<&CodeSnapshotFileEntry>,
) -> CommandResult<Option<CodePatchFileInput>> {
    patch_file_input_from_entries_with_hunk_budget(
        repo_root,
        project_id,
        file_index,
        requested_operation,
        before_entry,
        after_entry,
        DEFAULT_TEXT_HUNK_PAYLOAD_BYTES,
    )
}

fn patch_file_input_from_entries_with_hunk_budget(
    repo_root: &Path,
    project_id: &str,
    file_index: usize,
    requested_operation: CodeFileOperation,
    before_entry: Option<&CodeSnapshotFileEntry>,
    after_entry: Option<&CodeSnapshotFileEntry>,
    max_text_hunk_payload_bytes: usize,
) -> CommandResult<Option<CodePatchFileInput>> {
    let Some(operation) =
        patch_operation_from_entries(requested_operation, before_entry, after_entry)
    else {
        return Ok(None);
    };
    let patch_file_id = format!(
        "{}-{}",
        generate_id("code-patch-file"),
        file_index.saturating_add(1)
    );
    let (merge_policy, hunks) = patch_merge_policy_and_hunks(
        repo_root,
        project_id,
        &patch_file_id,
        operation,
        before_entry,
        after_entry,
        max_text_hunk_payload_bytes,
    )?;

    Ok(Some(CodePatchFileInput {
        patch_file_id,
        path_before: before_entry.map(|entry| entry.path.clone()),
        path_after: after_entry.map(|entry| entry.path.clone()),
        operation,
        merge_policy,
        before_file_kind: before_entry.map(patch_file_kind_from_snapshot),
        after_file_kind: after_entry.map(patch_file_kind_from_snapshot),
        base_hash: before_entry.and_then(|entry| entry.sha256.clone()),
        result_hash: after_entry.and_then(|entry| entry.sha256.clone()),
        base_blob_id: before_entry.and_then(|entry| entry.blob_id.clone()),
        result_blob_id: after_entry.and_then(|entry| entry.blob_id.clone()),
        base_size: before_entry.and_then(|entry| entry.size),
        result_size: after_entry.and_then(|entry| entry.size),
        base_mode: before_entry.and_then(|entry| entry.mode),
        result_mode: after_entry.and_then(|entry| entry.mode),
        base_symlink_target: before_entry.and_then(|entry| entry.symlink_target.clone()),
        result_symlink_target: after_entry.and_then(|entry| entry.symlink_target.clone()),
        hunks,
    }))
}

fn patch_operation_from_entries(
    requested_operation: CodeFileOperation,
    before_entry: Option<&CodeSnapshotFileEntry>,
    after_entry: Option<&CodeSnapshotFileEntry>,
) -> Option<CodePatchFileOperation> {
    match (before_entry, after_entry) {
        (None, None) => None,
        (None, Some(_)) => Some(CodePatchFileOperation::Create),
        (Some(_), None) => Some(CodePatchFileOperation::Delete),
        (Some(before), Some(after)) if before.path != after.path => {
            Some(CodePatchFileOperation::Rename)
        }
        (Some(_), Some(_)) => Some(match requested_operation {
            CodeFileOperation::Create | CodeFileOperation::Modify | CodeFileOperation::Delete => {
                CodePatchFileOperation::Modify
            }
            CodeFileOperation::Rename => CodePatchFileOperation::Rename,
            CodeFileOperation::ModeChange => CodePatchFileOperation::ModeChange,
            CodeFileOperation::SymlinkChange => CodePatchFileOperation::SymlinkChange,
        }),
    }
}

fn patch_merge_policy_and_hunks(
    repo_root: &Path,
    project_id: &str,
    patch_file_id: &str,
    operation: CodePatchFileOperation,
    before_entry: Option<&CodeSnapshotFileEntry>,
    after_entry: Option<&CodeSnapshotFileEntry>,
    max_text_hunk_payload_bytes: usize,
) -> CommandResult<(CodePatchMergePolicy, Vec<CodePatchHunkInput>)> {
    if !matches!(
        operation,
        CodePatchFileOperation::Create
            | CodePatchFileOperation::Modify
            | CodePatchFileOperation::Delete
    ) {
        return Ok((CodePatchMergePolicy::Exact, Vec::new()));
    }
    if !entry_side_is_absent_or_file(before_entry) || !entry_side_is_absent_or_file(after_entry) {
        return Ok((CodePatchMergePolicy::Exact, Vec::new()));
    }

    let Some(before_text) = text_for_patch_entry(repo_root, project_id, before_entry)? else {
        return Ok((CodePatchMergePolicy::Exact, Vec::new()));
    };
    let Some(after_text) = text_for_patch_entry(repo_root, project_id, after_entry)? else {
        return Ok((CodePatchMergePolicy::Exact, Vec::new()));
    };
    if before_text == after_text {
        return Ok((CodePatchMergePolicy::Exact, Vec::new()));
    }

    let hunks = build_single_text_hunk(patch_file_id, &before_text, &after_text);
    if text_hunk_payload_bytes(&hunks) > max_text_hunk_payload_bytes {
        return Ok((CodePatchMergePolicy::Exact, Vec::new()));
    }

    Ok((CodePatchMergePolicy::Text, hunks))
}

fn entry_side_is_absent_or_file(entry: Option<&CodeSnapshotFileEntry>) -> bool {
    entry.map_or(true, |entry| entry.kind == CodeSnapshotFileKind::File)
}

fn text_for_patch_entry(
    repo_root: &Path,
    project_id: &str,
    entry: Option<&CodeSnapshotFileEntry>,
) -> CommandResult<Option<String>> {
    let Some(entry) = entry else {
        return Ok(Some(String::new()));
    };
    if entry.kind != CodeSnapshotFileKind::File {
        return Ok(None);
    }
    let Some(blob_id) = entry.blob_id.as_deref() else {
        return Ok(None);
    };
    let (connection, _database_path) = open_code_rollback_database(repo_root)?;
    let bytes = read_blob_bytes(&connection, repo_root, project_id, blob_id)?;
    if bytes.contains(&0) {
        return Ok(None);
    }
    String::from_utf8(bytes).map(Some).map_err(|error| {
        CommandError::system_fault(
            "code_patch_text_decode_failed",
            format!(
                "Xero could not decode text blob `{blob_id}` for code history patch capture: {error}"
            ),
        )
    })
}

fn build_single_text_hunk(
    patch_file_id: &str,
    before_text: &str,
    after_text: &str,
) -> Vec<CodePatchHunkInput> {
    let before_lines = split_text_lines_preserving_endings(before_text);
    let after_lines = split_text_lines_preserving_endings(after_text);
    let mut prefix_len = 0;
    while before_lines.get(prefix_len) == after_lines.get(prefix_len)
        && prefix_len < before_lines.len()
        && prefix_len < after_lines.len()
    {
        prefix_len += 1;
    }

    let mut suffix_len = 0;
    while suffix_len < before_lines.len().saturating_sub(prefix_len)
        && suffix_len < after_lines.len().saturating_sub(prefix_len)
        && before_lines[before_lines.len() - 1 - suffix_len]
            == after_lines[after_lines.len() - 1 - suffix_len]
    {
        suffix_len += 1;
    }

    let before_end = before_lines.len().saturating_sub(suffix_len);
    let after_end = after_lines.len().saturating_sub(suffix_len);
    let context_before_start = prefix_len.saturating_sub(3);
    let context_after_end = (before_end + 3).min(before_lines.len());

    vec![CodePatchHunkInput {
        hunk_id: format!("{}-{}", generate_id("code-hunk"), patch_file_id),
        hunk_index: 0,
        base_start_line: start_line_for_hunk(prefix_len, before_lines.len()),
        base_line_count: (before_end - prefix_len).min(u32::MAX as usize) as u32,
        result_start_line: start_line_for_hunk(prefix_len, after_lines.len()),
        result_line_count: (after_end - prefix_len).min(u32::MAX as usize) as u32,
        removed_lines: before_lines[prefix_len..before_end].to_vec(),
        added_lines: after_lines[prefix_len..after_end].to_vec(),
        context_before: before_lines[context_before_start..prefix_len].to_vec(),
        context_after: before_lines[before_end..context_after_end].to_vec(),
    }]
}

fn text_hunk_payload_bytes(hunks: &[CodePatchHunkInput]) -> usize {
    hunks.iter().fold(0usize, |total, hunk| {
        total
            .saturating_add(line_payload_bytes(&hunk.removed_lines))
            .saturating_add(line_payload_bytes(&hunk.added_lines))
            .saturating_add(line_payload_bytes(&hunk.context_before))
            .saturating_add(line_payload_bytes(&hunk.context_after))
    })
}

fn line_payload_bytes(lines: &[String]) -> usize {
    lines
        .iter()
        .fold(0usize, |total, line| total.saturating_add(line.len()))
}

fn split_text_lines_preserving_endings(text: &str) -> Vec<String> {
    text.split_inclusive('\n')
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>()
}

fn start_line_for_hunk(prefix_len: usize, total_lines: usize) -> u32 {
    if total_lines == 0 {
        0
    } else {
        prefix_len.saturating_add(1).min(u32::MAX as usize) as u32
    }
}

fn patch_file_kind_from_snapshot(entry: &CodeSnapshotFileEntry) -> CodePatchFileKind {
    match entry.kind {
        CodeSnapshotFileKind::File => CodePatchFileKind::File,
        CodeSnapshotFileKind::Directory => CodePatchFileKind::Directory,
        CodeSnapshotFileKind::Symlink => CodePatchFileKind::Symlink,
    }
}

fn load_completed_change_group_commit_metadata(
    repo_root: &Path,
    project_id: &str,
    change_group_id: &str,
) -> CommandResult<CompletedChangeGroupCommitMetadata> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    let row = connection
        .query_row(
            r#"
            SELECT
                tool_call_id,
                runtime_event_id,
                conversation_sequence,
                change_kind,
                summary_label,
                status,
                started_at,
                completed_at
            FROM code_change_groups
            WHERE project_id = ?1
              AND change_group_id = ?2
            "#,
            params![project_id, change_group_id],
            |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<i64>>(1)?,
                    row.get::<_, Option<i64>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, Option<String>>(7)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "code_change_group_query_failed",
                format!(
                    "Xero could not query code change group `{change_group_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "code_change_group_missing",
                format!("Xero could not find code change group `{change_group_id}`."),
            )
        })?;
    let (
        tool_call_id,
        runtime_event_id,
        conversation_sequence,
        change_kind,
        summary_label,
        status,
        started_at,
        completed_at,
    ) = row;
    if status != "completed" {
        return Err(CommandError::retryable(
            "code_change_group_not_complete",
            format!(
                "Code change group `{change_group_id}` is `{status}` and cannot be committed to code history yet."
            ),
        ));
    }
    let completed_at = completed_at.ok_or_else(|| {
        CommandError::system_fault(
            "code_change_group_completed_at_missing",
            format!("Completed code change group `{change_group_id}` is missing completed_at."),
        )
    })?;

    Ok(CompletedChangeGroupCommitMetadata {
        tool_call_id,
        runtime_event_id,
        conversation_sequence,
        change_kind,
        summary_label,
        started_at,
        completed_at,
    })
}

fn commit_kind_for_change_kind(change_kind: &str) -> CodeHistoryCommitKind {
    match change_kind {
        "recovered_mutation" => CodeHistoryCommitKind::RecoveredMutation,
        "imported_baseline" => CodeHistoryCommitKind::ImportedBaseline,
        _ => CodeHistoryCommitKind::ChangeGroup,
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TreeFingerprintEntry<'a> {
    path: &'a str,
    kind: &'static str,
    size: Option<u64>,
    sha256: Option<&'a str>,
    mode: Option<u32>,
    symlink_target: Option<&'a str>,
}

fn code_tree_id_for_manifest(manifest: &CodeSnapshotManifest) -> CommandResult<String> {
    let entry_map = manifest.entry_map();
    let entries = entry_map
        .values()
        .map(|entry| TreeFingerprintEntry {
            path: entry.path.as_str(),
            kind: entry.kind.as_str(),
            size: entry.size,
            sha256: entry.sha256.as_deref(),
            mode: entry.mode,
            symlink_target: entry.symlink_target.as_deref(),
        })
        .collect::<Vec<_>>();
    let encoded = serde_json::to_vec(&entries).map_err(|error| {
        CommandError::system_fault(
            "code_tree_fingerprint_encode_failed",
            format!("Xero could not encode code tree fingerprint: {error}"),
        )
    })?;
    Ok(format!("code-tree-{}", sha256_hex(&encoded)))
}

fn affected_paths_for_patch_files(files: &[CodePatchFileInput]) -> Vec<String> {
    files
        .iter()
        .flat_map(|file| [file.path_before.as_deref(), file.path_after.as_deref()])
        .flatten()
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn completed_file_from_entries(
    target: &CodeRollbackCaptureTarget,
    operation: CodeFileOperation,
    before_entry: Option<&CodeSnapshotFileEntry>,
    after_entry: Option<&CodeSnapshotFileEntry>,
) -> CompletedCodeChangeFile {
    CompletedCodeChangeFile {
        path_before: target.path_before.clone(),
        path_after: target.path_after.clone(),
        operation,
        before_hash: before_entry.and_then(|entry| entry.sha256.clone()),
        after_hash: after_entry.and_then(|entry| entry.sha256.clone()),
        explicitly_edited: target.explicitly_edited,
    }
}

fn insert_file_version(
    tx: &Transaction<'_>,
    project_id: &str,
    change_group_id: &str,
    path_before: Option<&str>,
    path_after: Option<&str>,
    operation: CodeFileOperation,
    before_entry: Option<&CodeSnapshotFileEntry>,
    after_entry: Option<&CodeSnapshotFileEntry>,
    explicitly_edited: bool,
) -> CommandResult<()> {
    let generated = path_before
        .or(path_after)
        .is_some_and(is_generated_or_ignored_path);
    tx.execute(
        r#"
        INSERT INTO code_file_versions (
            project_id,
            change_group_id,
            path_before,
            path_after,
            operation,
            before_file_kind,
            after_file_kind,
            before_hash,
            after_hash,
            before_blob_id,
            after_blob_id,
            before_size,
            after_size,
            before_mode,
            after_mode,
            before_symlink_target,
            after_symlink_target,
            explicitly_edited,
            generated,
            created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)
        "#,
        params![
            project_id,
            change_group_id,
            path_before,
            path_after,
            operation.as_str(),
            before_entry.map(|entry| entry.kind.as_str()),
            after_entry.map(|entry| entry.kind.as_str()),
            before_entry.and_then(|entry| entry.sha256.as_deref()),
            after_entry.and_then(|entry| entry.sha256.as_deref()),
            before_entry.and_then(|entry| entry.blob_id.as_deref()),
            after_entry.and_then(|entry| entry.blob_id.as_deref()),
            before_entry.and_then(|entry| entry.size).map(saturating_u64_to_i64),
            after_entry.and_then(|entry| entry.size).map(saturating_u64_to_i64),
            before_entry.and_then(|entry| entry.mode).map(i64::from),
            after_entry.and_then(|entry| entry.mode).map(i64::from),
            before_entry.and_then(|entry| entry.symlink_target.as_deref()),
            after_entry.and_then(|entry| entry.symlink_target.as_deref()),
            if explicitly_edited { 1 } else { 0 },
            if generated { 1 } else { 0 },
            now_timestamp(),
        ],
    )
    .map_err(|error| {
        CommandError::system_fault(
            "code_file_version_insert_failed",
            format!("Xero could not persist code file version metadata: {error}"),
        )
    })?;
    Ok(())
}

fn complete_change_group_tx(
    tx: &Transaction<'_>,
    project_id: &str,
    change_group_id: &str,
    after_snapshot_id: &str,
) -> CommandResult<()> {
    let completed_at = now_timestamp();
    tx.execute(
        r#"
        UPDATE code_change_groups
        SET after_snapshot_id = ?3,
            status = 'completed',
            completed_at = ?4
        WHERE project_id = ?1
          AND change_group_id = ?2
        "#,
        params![project_id, change_group_id, after_snapshot_id, completed_at],
    )
    .map_err(|error| {
        CommandError::system_fault(
            "code_change_group_complete_failed",
            format!("Xero could not complete code change group `{change_group_id}`: {error}"),
        )
    })?;
    Ok(())
}

fn insert_pending_snapshot(
    connection: &Connection,
    repo_root: &Path,
    request: &SnapshotCaptureRequest,
    snapshot_id: &str,
) -> CommandResult<()> {
    let created_at = now_timestamp();
    connection
        .execute(
            r#"
            INSERT INTO code_snapshots (
                project_id,
                agent_session_id,
                run_id,
                snapshot_id,
                change_group_id,
                boundary_kind,
                root_path,
                write_state,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)
            "#,
            params![
                request.project_id,
                request.agent_session_id,
                request.run_id,
                snapshot_id,
                request.change_group_id,
                request.boundary_kind.as_str(),
                repo_root.to_string_lossy(),
                created_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_snapshot_insert_failed",
                format!("Xero could not begin code snapshot `{snapshot_id}`: {error}"),
            )
        })?;
    Ok(())
}

fn complete_snapshot(
    connection: &Connection,
    project_id: &str,
    snapshot_id: &str,
    manifest: &CodeSnapshotManifest,
) -> CommandResult<()> {
    let manifest_json = serde_json::to_string(manifest).map_err(|error| {
        CommandError::system_fault(
            "code_snapshot_manifest_encode_failed",
            format!("Xero could not encode code snapshot manifest: {error}"),
        )
    })?;
    let completed_at = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE code_snapshots
            SET manifest_json = ?3,
                write_state = 'completed',
                entry_count = ?4,
                total_file_bytes = ?5,
                completed_at = ?6
            WHERE project_id = ?1
              AND snapshot_id = ?2
            "#,
            params![
                project_id,
                snapshot_id,
                manifest_json,
                saturating_usize_to_i64(manifest.entries.len()),
                saturating_u64_to_i64(manifest.total_file_bytes()),
                completed_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_snapshot_complete_failed",
                format!("Xero could not complete code snapshot `{snapshot_id}`: {error}"),
            )
        })?;
    Ok(())
}

fn mark_snapshot_failed(
    connection: &Connection,
    project_id: &str,
    snapshot_id: &str,
    diagnostic: JsonValue,
) -> CommandResult<()> {
    let diagnostic_json = serde_json::to_string(&diagnostic).map_err(|error| {
        CommandError::system_fault(
            "code_snapshot_diagnostic_encode_failed",
            format!("Xero could not encode code snapshot diagnostic: {error}"),
        )
    })?;
    let completed_at = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE code_snapshots
            SET write_state = 'failed',
                diagnostic_json = ?3,
                completed_at = COALESCE(completed_at, ?4)
            WHERE project_id = ?1
              AND snapshot_id = ?2
            "#,
            params![project_id, snapshot_id, diagnostic_json, completed_at],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_snapshot_mark_failed_failed",
                format!("Xero could not mark code snapshot `{snapshot_id}` failed: {error}"),
            )
        })?;
    Ok(())
}

fn mark_change_group_failed(
    repo_root: &Path,
    project_id: &str,
    change_group_id: &str,
    error: &CommandError,
) -> CommandResult<()> {
    let (connection, _database_path) = open_code_rollback_database(repo_root)?;
    let diagnostic_json = serde_json::to_string(&json!({
        "code": error.code,
        "message": error.message,
    }))
    .map_err(|error| {
        CommandError::system_fault(
            "code_change_group_diagnostic_encode_failed",
            format!("Xero could not encode code change group diagnostic: {error}"),
        )
    })?;
    let completed_at = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE code_change_groups
            SET status = 'failed',
                diagnostic_json = ?3,
                completed_at = COALESCE(completed_at, ?4)
            WHERE project_id = ?1
              AND change_group_id = ?2
            "#,
            params![project_id, change_group_id, diagnostic_json, completed_at],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_change_group_mark_failed_failed",
                format!(
                    "Xero could not mark code change group `{change_group_id}` failed: {error}"
                ),
            )
        })?;
    Ok(())
}

fn project_restore_lock(repo_root: &Path, project_id: &str) -> CommandResult<Arc<Mutex<()>>> {
    let root_key = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf())
        .to_string_lossy()
        .into_owned();
    let key = format!("{root_key}\u{0}{project_id}");
    let mut locks = CODE_RESTORE_LOCKS.lock().map_err(|_| {
        CommandError::system_fault(
            "code_rollback_lock_registry_poisoned",
            "Xero could not prepare the code rollback restore lane.",
        )
    })?;
    Ok(locks
        .entry(key)
        .or_insert_with(|| Arc::new(Mutex::new(())))
        .clone())
}

fn load_rollback_target_change_group(
    repo_root: &Path,
    project_id: &str,
    target_change_group_id: &str,
) -> CommandResult<RollbackTargetChangeGroup> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let target = connection
        .query_row(
            r#"
            SELECT
                agent_session_id,
                run_id,
                change_group_id,
                before_snapshot_id,
                summary_label,
                restore_state,
                status
            FROM code_change_groups
            WHERE project_id = ?1
              AND change_group_id = ?2
            "#,
            params![project_id, target_change_group_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<String>>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, String>(6)?,
                ))
            },
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "code_change_group_query_failed",
                format!(
                    "Xero could not query code change group `{target_change_group_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "code_change_group_missing",
                format!("Xero could not find code change group `{target_change_group_id}`."),
            )
        })?;
    let (
        agent_session_id,
        run_id,
        change_group_id,
        before_snapshot_id,
        summary_label,
        restore_state,
        status,
    ) = target;
    if restore_state != CodeChangeRestoreState::SnapshotAvailable.as_str() {
        return Err(CommandError::user_fixable(
            "code_change_group_not_restorable",
            format!(
                "Code change group `{target_change_group_id}` is `{restore_state}` and cannot be restored."
            ),
        ));
    }
    if !matches!(status.as_str(), "completed" | "rolled_back") {
        return Err(CommandError::retryable(
            "code_change_group_not_complete",
            format!(
                "Code change group `{target_change_group_id}` is `{status}` and cannot be restored."
            ),
        ));
    }
    let before_snapshot_id = before_snapshot_id.ok_or_else(|| {
        CommandError::retryable(
            "code_change_group_snapshot_missing",
            format!(
                "Code change group `{target_change_group_id}` does not have a before snapshot."
            ),
        )
    })?;
    Ok(RollbackTargetChangeGroup {
        project_id: project_id.into(),
        agent_session_id,
        run_id,
        change_group_id,
        before_snapshot_id,
        summary_label,
    })
}

fn applied_code_rollback_from_change_group_undo(
    repo_root: &Path,
    target: RollbackTargetChangeGroup,
    applied: AppliedCodeChangeGroupUndo,
) -> CommandResult<AppliedCodeRollback> {
    let result_change_group_id = applied.result_change_group_id.clone().ok_or_else(|| {
        CommandError::system_fault(
            "code_rollback_result_missing",
            format!(
                "Undo operation `{}` completed without a result change group.",
                applied.operation_id
            ),
        )
    })?;
    let pre_rollback_snapshot_id = load_change_group_before_snapshot_id(
        repo_root,
        &target.project_id,
        &result_change_group_id,
    )?;

    complete_rollback_operation(
        repo_root,
        &target.project_id,
        &applied.operation_id,
        &pre_rollback_snapshot_id,
        &result_change_group_id,
        &applied.affected_files,
    )?;

    Ok(AppliedCodeRollback {
        project_id: target.project_id,
        agent_session_id: target.agent_session_id,
        run_id: target.run_id,
        operation_id: applied.operation_id,
        target_change_group_id: target.change_group_id,
        target_snapshot_id: target.before_snapshot_id,
        pre_rollback_snapshot_id,
        result_change_group_id,
        restored_paths: restored_paths_from_affected_files(&applied.affected_files),
        removed_paths: removed_paths_from_affected_files(&applied.affected_files),
        affected_files: applied.affected_files,
    })
}

fn load_change_group_before_snapshot_id(
    repo_root: &Path,
    project_id: &str,
    change_group_id: &str,
) -> CommandResult<String> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    connection
        .query_row(
            r#"
            SELECT before_snapshot_id
            FROM code_change_groups
            WHERE project_id = ?1
              AND change_group_id = ?2
            "#,
            params![project_id, change_group_id],
            |row| row.get::<_, Option<String>>(0),
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "code_change_group_query_failed",
                format!(
                    "Xero could not query result change group `{change_group_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?
        .flatten()
        .ok_or_else(|| {
            CommandError::system_fault(
                "code_rollback_pre_snapshot_missing",
                format!(
                    "Result change group `{change_group_id}` does not have a pre-undo snapshot."
                ),
            )
        })
}

fn insert_pending_rollback_operation(
    repo_root: &Path,
    operation_id: &str,
    target: &RollbackTargetChangeGroup,
    result_change_group_id: Option<&str>,
) -> CommandResult<()> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    connection
        .execute(
            r#"
            INSERT INTO code_rollback_operations (
                project_id,
                operation_id,
                agent_session_id,
                run_id,
                target_change_group_id,
                target_snapshot_id,
                result_change_group_id,
                status,
                created_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)
            "#,
            params![
                target.project_id.as_str(),
                operation_id,
                target.agent_session_id.as_str(),
                target.run_id.as_str(),
                target.change_group_id.as_str(),
                target.before_snapshot_id.as_str(),
                result_change_group_id,
                now_timestamp(),
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_insert_failed",
                format!(
                    "Xero could not record pending rollback operation `{operation_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

fn complete_rollback_operation(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    pre_rollback_snapshot_id: &str,
    result_change_group_id: &str,
    affected_files: &[CompletedCodeChangeFile],
) -> CommandResult<()> {
    let affected_files_json = affected_files_json(affected_files)?;
    let completed_at = now_timestamp();
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE code_rollback_operations
            SET status = 'completed',
                pre_rollback_snapshot_id = ?3,
                result_change_group_id = ?4,
                affected_files_json = ?5,
                completed_at = ?6
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![
                project_id,
                operation_id,
                pre_rollback_snapshot_id,
                result_change_group_id,
                affected_files_json,
                completed_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_complete_failed",
                format!(
                    "Xero could not complete rollback operation `{operation_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

fn mark_rollback_operation_failed_best_effort(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    error: &CommandError,
) {
    if let Ok((connection, _database_path)) = open_code_rollback_database(repo_root) {
        let _ = mark_rollback_operation_failed(&connection, project_id, operation_id, error);
    }
}

fn mark_rollback_operation_failed(
    connection: &Connection,
    project_id: &str,
    operation_id: &str,
    error: &CommandError,
) -> CommandResult<()> {
    let completed_at = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE code_rollback_operations
            SET status = 'failed',
                failure_code = ?3,
                failure_message = ?4,
                completed_at = COALESCE(completed_at, ?5)
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![
                project_id,
                operation_id,
                error.code.as_str(),
                error.message.as_str(),
                completed_at,
            ],
        )
        .map_err(|update_error| {
            CommandError::system_fault(
                "code_rollback_operation_mark_failed_failed",
                format!(
                    "Xero could not mark rollback operation `{operation_id}` failed: {update_error}"
                ),
            )
        })?;
    Ok(())
}

fn code_rollback_conflict_error(
    target_change_group_id: &str,
    conflicts: &[CodeFileUndoConflict],
) -> CommandError {
    let summary = conflicts
        .iter()
        .take(3)
        .map(|conflict| format!("{} ({})", conflict.path, conflict.kind.as_str()))
        .collect::<Vec<_>>()
        .join(", ");
    let remainder = conflicts.len().saturating_sub(3);
    let suffix = if remainder == 0 {
        String::new()
    } else {
        format!(" and {remainder} more")
    };
    let detail = if summary.is_empty() {
        "current workspace changes conflict with the selected change".to_string()
    } else {
        format!("current workspace changes conflict at {summary}{suffix}")
    };
    CommandError::user_fixable(
        "code_undo_conflicted",
        format!(
            "Code change group `{target_change_group_id}` could not be undone because {detail}. No files were changed."
        ),
    )
}

fn restored_paths_from_affected_files(affected_files: &[CompletedCodeChangeFile]) -> Vec<String> {
    affected_files
        .iter()
        .filter_map(|file| file.path_after.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn removed_paths_from_affected_files(affected_files: &[CompletedCodeChangeFile]) -> Vec<String> {
    affected_files
        .iter()
        .filter(|file| file.path_after.is_none())
        .filter_map(|file| file.path_before.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn affected_files_json(affected_files: &[CompletedCodeChangeFile]) -> CommandResult<String> {
    let files = affected_files
        .iter()
        .map(|file| {
            json!({
                "pathBefore": file.path_before.as_deref(),
                "pathAfter": file.path_after.as_deref(),
                "operation": file.operation.as_str(),
                "beforeHash": file.before_hash.as_deref(),
                "afterHash": file.after_hash.as_deref(),
                "explicitlyEdited": file.explicitly_edited,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&files).map_err(|error| {
        CommandError::system_fault(
            "code_rollback_affected_files_encode_failed",
            format!("Xero could not encode rollback affected files: {error}"),
        )
    })
}

fn begin_code_history_operation(
    repo_root: &Path,
    start: &CodeHistoryOperationStart,
) -> CommandResult<CodeHistoryOperationBegin> {
    validate_non_empty(&start.project_id, "projectId")?;
    validate_non_empty(&start.operation_id, "operationId")?;
    validate_non_empty(&start.target_id, "targetId")?;
    if let Some(change_group_id) = start.target_change_group_id.as_deref() {
        validate_non_empty(change_group_id, "targetChangeGroupId")?;
    }
    if let Some(file_path) = start.target_file_path.as_deref() {
        validate_non_empty(file_path, "targetFilePath")?;
    }
    for hunk_id in &start.target_hunk_ids {
        validate_non_empty(hunk_id, "targetHunkId")?;
    }
    let affected_paths = normalize_history_paths(&start.affected_paths)?;
    let target_hunk_ids_json = string_list_json(&start.target_hunk_ids)?;
    let affected_paths_json = string_list_json(&affected_paths)?;
    let expected_workspace_epoch = start
        .expected_workspace_epoch
        .map(|epoch| u64_to_i64(epoch, "expectedWorkspaceEpoch"))
        .transpose()?;
    let now = now_timestamp();
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    if let Some(existing) = read_existing_code_history_operation(
        &connection,
        &database_path,
        &start.project_id,
        &start.operation_id,
    )? {
        return Ok(CodeHistoryOperationBegin::Existing(existing));
    }

    connection
        .execute(
            r#"
            INSERT INTO code_history_operations (
                project_id,
                operation_id,
                mode,
                status,
                target_kind,
                target_id,
                target_change_group_id,
                target_file_path,
                target_hunk_ids_json,
                agent_session_id,
                run_id,
                expected_workspace_epoch,
                affected_paths_json,
                conflicts_json,
                created_at,
                updated_at
            )
            VALUES (?1, ?2, ?3, 'planning', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, '[]', ?13, ?13)
            "#,
            params![
                start.project_id,
                start.operation_id,
                start.mode.as_str(),
                start.target_kind.as_str(),
                start.target_id,
                start.target_change_group_id,
                start.target_file_path,
                target_hunk_ids_json,
                start.agent_session_id,
                start.run_id,
                expected_workspace_epoch,
                affected_paths_json,
                now,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_insert_failed",
                format!(
                    "Xero could not record code history operation `{}` in {}: {error}",
                    start.operation_id,
                    database_path.display()
                ),
            )
        })?;

    Ok(CodeHistoryOperationBegin::Started)
}

fn reject_existing_code_history_operation(existing: ExistingCodeHistoryOperation) -> CommandError {
    let message = format!(
        "Code history operation `{}` is already `{}`. Refresh operation status instead of retrying with the same id.",
        existing.operation_id,
        existing.status.as_str()
    );
    if existing.status.is_terminal() {
        CommandError::user_fixable("code_history_operation_already_resolved", message)
    } else {
        CommandError::retryable("code_history_operation_in_progress", message)
    }
}

fn read_existing_code_history_operation(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    operation_id: &str,
) -> CommandResult<Option<ExistingCodeHistoryOperation>> {
    connection
        .query_row(
            r#"
            SELECT operation_id, status
            FROM code_history_operations
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![project_id, operation_id],
            |row| {
                let status = row.get::<_, String>(1)?;
                Ok((row.get::<_, String>(0)?, status))
            },
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!(
                    "Xero could not query code history operation `{operation_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?
        .map(|(operation_id, status)| {
            Ok(ExistingCodeHistoryOperation {
                operation_id,
                status: CodeHistoryOperationStatus::from_sql(&status)?,
            })
        })
        .transpose()
}

fn mark_code_history_operation_applying(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    result_change_group_id: &str,
    affected_paths: &[String],
) -> CommandResult<()> {
    let affected_paths_json = string_list_json(&normalize_history_paths(affected_paths)?)?;
    let updated_at = now_timestamp();
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE code_history_operations
            SET status = 'applying',
                affected_paths_json = ?3,
                result_change_group_id = ?4,
                updated_at = ?5
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![
                project_id,
                operation_id,
                affected_paths_json,
                result_change_group_id,
                updated_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_update_failed",
                format!(
                    "Xero could not mark code history operation `{operation_id}` applying in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

fn mark_code_history_operation_conflicted(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    affected_paths: &[String],
    conflicts: &[CodeFileUndoConflict],
) -> CommandResult<()> {
    let affected_paths_json = string_list_json(&normalize_history_paths(affected_paths)?)?;
    let conflicts_json = code_history_conflicts_json(conflicts)?;
    let completed_at = now_timestamp();
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE code_history_operations
            SET status = 'conflicted',
                affected_paths_json = ?3,
                conflicts_json = ?4,
                updated_at = ?5,
                completed_at = ?5
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![
                project_id,
                operation_id,
                affected_paths_json,
                conflicts_json,
                completed_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_update_failed",
                format!(
                    "Xero could not mark code history operation `{operation_id}` conflicted in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    publish_code_history_coordination_event(
        &connection,
        repo_root,
        project_id,
        operation_id,
        CodeHistoryOperationStatus::Conflicted,
        &completed_at,
    )?;
    Ok(())
}

fn mark_code_history_operation_completed(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    result_change_group_id: Option<&str>,
    result_commit_id: Option<&str>,
    affected_paths: &[String],
) -> CommandResult<()> {
    let affected_paths_json = string_list_json(&normalize_history_paths(affected_paths)?)?;
    let completed_at = now_timestamp();
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE code_history_operations
            SET status = 'completed',
                affected_paths_json = ?3,
                conflicts_json = '[]',
                result_change_group_id = COALESCE(?4, result_change_group_id),
                result_commit_id = ?5,
                failure_code = NULL,
                failure_message = NULL,
                repair_code = NULL,
                repair_message = NULL,
                updated_at = ?6,
                completed_at = COALESCE(completed_at, ?6)
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![
                project_id,
                operation_id,
                affected_paths_json,
                result_change_group_id,
                result_commit_id,
                completed_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_update_failed",
                format!(
                    "Xero could not mark code history operation `{operation_id}` completed in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    publish_code_history_coordination_event(
        &connection,
        repo_root,
        project_id,
        operation_id,
        CodeHistoryOperationStatus::Completed,
        &completed_at,
    )?;
    Ok(())
}

fn mark_code_history_operation_failed_best_effort(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    error: &CommandError,
) {
    if let Ok((connection, _database_path)) = open_code_rollback_database(repo_root) {
        let _ = mark_code_history_operation_failed(
            &connection,
            repo_root,
            project_id,
            operation_id,
            error,
        );
    }
}

fn mark_code_history_operation_failed(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    error: &CommandError,
) -> CommandResult<()> {
    let completed_at = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE code_history_operations
            SET status = 'failed',
                failure_code = ?3,
                failure_message = ?4,
                updated_at = ?5,
                completed_at = COALESCE(completed_at, ?5)
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![
                project_id,
                operation_id,
                error.code.as_str(),
                error.message.as_str(),
                completed_at,
            ],
        )
        .map_err(|update_error| {
            CommandError::system_fault(
                "code_history_operation_mark_failed_failed",
                format!(
                    "Xero could not mark code history operation `{operation_id}` failed: {update_error}"
                ),
            )
        })?;
    publish_code_history_coordination_event(
        connection,
        repo_root,
        project_id,
        operation_id,
        CodeHistoryOperationStatus::Failed,
        &completed_at,
    )?;
    Ok(())
}

fn mark_code_history_operation_repair_needed_best_effort(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    code: &'static str,
    message: impl Into<String>,
) {
    if let Ok((connection, _database_path)) = open_code_rollback_database(repo_root) {
        let _ = mark_code_history_operation_repair_needed(
            &connection,
            repo_root,
            project_id,
            operation_id,
            code,
            message.into(),
        );
    }
}

fn mark_code_history_operation_repair_needed(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    code: &'static str,
    message: String,
) -> CommandResult<()> {
    let completed_at = now_timestamp();
    connection
        .execute(
            r#"
            UPDATE code_history_operations
            SET status = 'repair_needed',
                repair_code = ?3,
                repair_message = ?4,
                updated_at = ?5,
                completed_at = COALESCE(completed_at, ?5)
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![project_id, operation_id, code, message, completed_at],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_mark_repair_needed_failed",
                format!(
                    "Xero could not mark code history operation `{operation_id}` repair-needed: {error}"
                ),
            )
        })?;
    publish_code_history_coordination_event(
        connection,
        repo_root,
        project_id,
        operation_id,
        CodeHistoryOperationStatus::RepairNeeded,
        &completed_at,
    )?;
    Ok(())
}

#[derive(Debug)]
struct CodeHistoryCoordinationEventRow {
    operation_id: String,
    mode: String,
    target_kind: String,
    target_id: String,
    target_change_group_id: Option<String>,
    target_file_path: Option<String>,
    target_hunk_ids_json: String,
    agent_session_id: Option<String>,
    run_id: Option<String>,
    expected_workspace_epoch: Option<i64>,
    affected_paths_json: String,
    conflicts_json: String,
    result_change_group_id: Option<String>,
    result_commit_id: Option<String>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    repair_code: Option<String>,
    repair_message: Option<String>,
    updated_at: String,
    completed_at: Option<String>,
}

fn publish_code_history_coordination_event(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    status: CodeHistoryOperationStatus,
    occurred_at: &str,
) -> CommandResult<()> {
    let Some(row) = read_code_history_coordination_event_row(connection, project_id, operation_id)?
    else {
        return Ok(());
    };
    let Some(run_id) = row.run_id.clone() else {
        return Ok(());
    };

    let target_hunk_ids =
        decode_code_history_operation_json(&row.target_hunk_ids_json, "targetHunkIds")?;
    let affected_paths =
        decode_code_history_operation_json(&row.affected_paths_json, "affectedPaths")?;
    let affected_path_list = code_history_json_string_list(&affected_paths, "affectedPaths")?;
    let conflicts = decode_code_history_operation_json(&row.conflicts_json, "conflicts")?;
    let affected_path_count = affected_path_list.len();
    let conflict_count = conflicts.as_array().map_or(0, Vec::len);
    let event_kind = code_history_coordination_event_kind(status);
    if code_history_coordination_event_already_published(
        connection,
        project_id,
        run_id.as_str(),
        event_kind,
        &row.operation_id,
        status.as_str(),
    )? {
        return Ok(());
    }

    let payload = json!({
        "operationId": row.operation_id.as_str(),
        "mode": row.mode.as_str(),
        "status": status.as_str(),
        "target": {
            "kind": row.target_kind.as_str(),
            "id": row.target_id.as_str(),
            "changeGroupId": row.target_change_group_id.as_deref(),
            "filePath": row.target_file_path.as_deref(),
            "hunkIds": target_hunk_ids,
        },
        "agentSessionId": row.agent_session_id.as_deref(),
        "runId": run_id.as_str(),
        "expectedWorkspaceEpoch": row.expected_workspace_epoch,
        "affectedPaths": &affected_path_list,
        "conflicts": conflicts,
        "resultChangeGroupId": row.result_change_group_id.as_deref(),
        "resultCommitId": row.result_commit_id.as_deref(),
        "failure": optional_code_history_issue_json(row.failure_code.clone(), row.failure_message.clone()),
        "repair": optional_code_history_issue_json(row.repair_code.clone(), row.repair_message.clone()),
        "updatedAt": row.updated_at.as_str(),
        "completedAt": row.completed_at.as_deref(),
    });
    let payload_json = serde_json::to_string(&payload).map_err(|error| {
        CommandError::system_fault(
            "code_history_coordination_payload_encode_failed",
            format!("Xero could not encode code history coordination event payload: {error}"),
        )
    })?;
    let expires_at = timestamp_plus_history_coordination_lease(occurred_at)?;
    let summary = code_history_coordination_summary(
        &payload["mode"],
        status,
        affected_path_count,
        conflict_count,
    );

    let inserted = connection
        .execute(
            r#"
            INSERT INTO agent_coordination_events (
                project_id,
                run_id,
                trace_id,
                event_kind,
                summary,
                payload_json,
                created_at,
                expires_at
            )
            SELECT
                ?1,
                ?2,
                agent_runs.trace_id,
                ?3,
                ?4,
                ?5,
                ?6,
                ?7
            FROM agent_runs
            WHERE agent_runs.project_id = ?1
              AND agent_runs.run_id = ?2
            "#,
            params![
                project_id,
                run_id,
                event_kind,
                summary,
                payload_json,
                occurred_at,
                expires_at,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_coordination_event_insert_failed",
                format!(
                    "Xero could not publish code history coordination event for operation `{operation_id}` in {}: {error}",
                    database_path_for_repo(repo_root).display()
                ),
            )
        })?;
    if inserted == 0 {
        return Ok(());
    }
    publish_code_history_mailbox_notices(
        connection,
        repo_root,
        project_id,
        &row,
        status,
        occurred_at,
        &affected_path_list,
        conflict_count,
    )?;
    invalidate_code_history_file_reservations(
        repo_root,
        project_id,
        &row.operation_id,
        status,
        occurred_at,
        &affected_path_list,
    )?;
    Ok(())
}

#[derive(Debug, Clone)]
struct CodeHistoryMailboxRecipient {
    run_id: String,
    agent_session_id: Option<String>,
    priority: AgentMailboxPriority,
    invalidates_reservation: bool,
    reasons: BTreeSet<String>,
    matching_paths: BTreeSet<String>,
}

fn invalidate_code_history_file_reservations(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    status: CodeHistoryOperationStatus,
    occurred_at: &str,
    affected_paths: &[String],
) -> CommandResult<()> {
    if !code_history_should_invalidate_reservations(status) || affected_paths.is_empty() {
        return Ok(());
    }
    super::invalidate_overlapping_agent_file_reservations(
        repo_root,
        &InvalidateAgentFileReservationsRequest {
            project_id: project_id.into(),
            history_operation_id: operation_id.into(),
            affected_paths: affected_paths.to_vec(),
            invalidated_at: occurred_at.into(),
        },
    )?;
    Ok(())
}

fn publish_code_history_mailbox_notices(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    row: &CodeHistoryCoordinationEventRow,
    status: CodeHistoryOperationStatus,
    occurred_at: &str,
    affected_paths: &[String],
    conflict_count: usize,
) -> CommandResult<()> {
    let Some(sender_run_id) = row.run_id.as_deref() else {
        return Ok(());
    };
    let Some(overlap_item_type) = code_history_overlap_mailbox_item_type(status) else {
        return Ok(());
    };
    if affected_paths.is_empty() {
        return Ok(());
    }

    let mut recipients = collect_code_history_overlap_mailbox_recipients(
        connection,
        project_id,
        sender_run_id,
        occurred_at,
        affected_paths,
    )?;
    if code_history_should_publish_project_mailbox_notice(status) {
        for (run_id, agent_session_id) in list_active_code_history_mailbox_runs(
            connection,
            project_id,
            sender_run_id,
            occurred_at,
        )? {
            recipients
                .entry(run_id.clone())
                .or_insert_with(|| CodeHistoryMailboxRecipient {
                    run_id,
                    agent_session_id: Some(agent_session_id),
                    priority: AgentMailboxPriority::Normal,
                    invalidates_reservation: false,
                    reasons: BTreeSet::from(["same-project history update".to_string()]),
                    matching_paths: BTreeSet::new(),
                });
        }
    }

    for recipient in recipients.into_values() {
        let item_type = if recipient.invalidates_reservation
            && code_history_should_invalidate_reservations(status)
        {
            AgentMailboxItemType::ReservationInvalidated
        } else if recipient.priority == AgentMailboxPriority::High {
            overlap_item_type
        } else {
            AgentMailboxItemType::WorkspaceEpochAdvanced
        };
        insert_code_history_mailbox_notice(
            connection,
            repo_root,
            &CodeHistoryMailboxNotice {
                project_id,
                operation_id: row.operation_id.as_str(),
                sender_run_id,
                target_agent_session_id: recipient.agent_session_id.as_deref(),
                target_run_id: recipient.run_id.as_str(),
                item_type,
                priority: recipient.priority,
                title: code_history_mailbox_title(row.mode.as_str(), status, recipient.priority),
                body: code_history_mailbox_body(
                    row,
                    status,
                    affected_paths,
                    conflict_count,
                    &recipient,
                ),
                related_paths: affected_paths,
                created_at: occurred_at,
            },
        )?;
    }

    Ok(())
}

fn collect_code_history_overlap_mailbox_recipients(
    connection: &Connection,
    project_id: &str,
    sender_run_id: &str,
    occurred_at: &str,
    affected_paths: &[String],
) -> CommandResult<BTreeMap<String, CodeHistoryMailboxRecipient>> {
    let mut recipients = BTreeMap::new();
    collect_code_history_reservation_mailbox_recipients(
        connection,
        project_id,
        sender_run_id,
        occurred_at,
        affected_paths,
        &mut recipients,
    )?;
    collect_code_history_recent_path_mailbox_recipients(
        connection,
        project_id,
        sender_run_id,
        occurred_at,
        affected_paths,
        &mut recipients,
    )?;
    Ok(recipients)
}

fn collect_code_history_reservation_mailbox_recipients(
    connection: &Connection,
    project_id: &str,
    sender_run_id: &str,
    occurred_at: &str,
    affected_paths: &[String],
    recipients: &mut BTreeMap<String, CodeHistoryMailboxRecipient>,
) -> CommandResult<()> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                COALESCE(owner_child_run_id, owner_run_id) AS target_run_id,
                owner_agent_session_id,
                path
            FROM agent_file_reservations
            WHERE project_id = ?1
              AND released_at IS NULL
              AND expires_at > ?2
              AND COALESCE(owner_child_run_id, owner_run_id) <> ?3
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_reservation_query_failed",
                format!("Xero could not prepare history mailbox reservation lookup: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id, occurred_at, sender_run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_reservation_query_failed",
                format!(
                    "Xero could not query active file reservations for history notices: {error}"
                ),
            )
        })?;
    for row in rows {
        let (run_id, agent_session_id, reservation_path) = row.map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_reservation_query_failed",
                format!(
                    "Xero could not decode active file reservation for history notices: {error}"
                ),
            )
        })?;
        for affected_path in affected_paths {
            if coordination_paths_overlap(affected_path, &reservation_path) {
                upsert_code_history_mailbox_recipient(
                    recipients,
                    run_id.as_str(),
                    Some(agent_session_id.as_str()),
                    AgentMailboxPriority::High,
                    "overlapping reservation",
                    affected_path,
                    true,
                );
            }
        }
    }
    Ok(())
}

fn collect_code_history_recent_path_mailbox_recipients(
    connection: &Connection,
    project_id: &str,
    sender_run_id: &str,
    occurred_at: &str,
    affected_paths: &[String],
    recipients: &mut BTreeMap<String, CodeHistoryMailboxRecipient>,
) -> CommandResult<()> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                presence.run_id,
                presence.agent_session_id,
                json_extract(events.payload_json, '$.path') AS path
            FROM agent_coordination_presence AS presence
            JOIN agent_coordination_events AS events
              ON events.project_id = presence.project_id
             AND events.run_id = presence.run_id
            WHERE presence.project_id = ?1
              AND presence.expires_at > ?2
              AND events.expires_at > ?2
              AND presence.run_id <> ?3
              AND json_type(events.payload_json, '$.path') = 'text'
            ORDER BY events.created_at DESC, events.id DESC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_activity_query_failed",
                format!("Xero could not prepare history mailbox activity lookup: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id, occurred_at, sender_run_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_activity_query_failed",
                format!("Xero could not query recent path activity for history notices: {error}"),
            )
        })?;
    for row in rows {
        let (run_id, agent_session_id, recent_path) = row.map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_activity_query_failed",
                format!("Xero could not decode recent path activity for history notices: {error}"),
            )
        })?;
        for affected_path in affected_paths {
            if coordination_paths_overlap(affected_path, &recent_path) {
                upsert_code_history_mailbox_recipient(
                    recipients,
                    run_id.as_str(),
                    Some(agent_session_id.as_str()),
                    AgentMailboxPriority::High,
                    "recent path activity",
                    affected_path,
                    false,
                );
            }
        }
    }
    Ok(())
}

fn list_active_code_history_mailbox_runs(
    connection: &Connection,
    project_id: &str,
    sender_run_id: &str,
    occurred_at: &str,
) -> CommandResult<Vec<(String, String)>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT run_id, agent_session_id
            FROM agent_coordination_presence
            WHERE project_id = ?1
              AND expires_at > ?2
              AND run_id <> ?3
            ORDER BY updated_at DESC, run_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_presence_query_failed",
                format!("Xero could not prepare active-run history mailbox lookup: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id, occurred_at, sender_run_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_presence_query_failed",
                format!("Xero could not query active runs for history notices: {error}"),
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        CommandError::system_fault(
            "code_history_mailbox_presence_query_failed",
            format!("Xero could not decode active runs for history notices: {error}"),
        )
    })
}

fn upsert_code_history_mailbox_recipient(
    recipients: &mut BTreeMap<String, CodeHistoryMailboxRecipient>,
    run_id: &str,
    agent_session_id: Option<&str>,
    priority: AgentMailboxPriority,
    reason: &str,
    matching_path: &str,
    invalidates_reservation: bool,
) {
    let recipient =
        recipients
            .entry(run_id.to_string())
            .or_insert_with(|| CodeHistoryMailboxRecipient {
                run_id: run_id.to_string(),
                agent_session_id: agent_session_id.map(ToOwned::to_owned),
                priority,
                invalidates_reservation,
                reasons: BTreeSet::new(),
                matching_paths: BTreeSet::new(),
            });
    if recipient.agent_session_id.is_none() {
        recipient.agent_session_id = agent_session_id.map(ToOwned::to_owned);
    }
    if mailbox_priority_rank(priority) < mailbox_priority_rank(recipient.priority) {
        recipient.priority = priority;
    }
    recipient.invalidates_reservation |= invalidates_reservation;
    recipient.reasons.insert(reason.to_string());
    recipient.matching_paths.insert(matching_path.to_string());
}

struct CodeHistoryMailboxNotice<'a> {
    project_id: &'a str,
    operation_id: &'a str,
    sender_run_id: &'a str,
    target_agent_session_id: Option<&'a str>,
    target_run_id: &'a str,
    item_type: AgentMailboxItemType,
    priority: AgentMailboxPriority,
    title: String,
    body: String,
    related_paths: &'a [String],
    created_at: &'a str,
}

fn insert_code_history_mailbox_notice(
    connection: &Connection,
    repo_root: &Path,
    notice: &CodeHistoryMailboxNotice<'_>,
) -> CommandResult<()> {
    let item_id = code_history_mailbox_item_id(
        notice.operation_id,
        notice.item_type.as_str(),
        notice.target_run_id,
    );
    if code_history_mailbox_notice_exists(connection, notice.project_id, &item_id)? {
        return Ok(());
    }
    let related_paths_json = string_list_json(notice.related_paths)?;
    let expires_at = timestamp_plus_history_coordination_lease(notice.created_at)?;
    let inserted = connection
        .execute(
            r#"
            INSERT INTO agent_mailbox_items (
                item_id,
                project_id,
                item_type,
                parent_item_id,
                sender_agent_session_id,
                sender_run_id,
                sender_parent_run_id,
                sender_child_run_id,
                sender_role,
                sender_trace_id,
                target_agent_session_id,
                target_run_id,
                target_role,
                title,
                body,
                related_paths_json,
                priority,
                status,
                created_at,
                expires_at
            )
            SELECT
                ?1,
                ?2,
                ?3,
                NULL,
                agent_runs.agent_session_id,
                agent_runs.run_id,
                agent_runs.parent_run_id,
                CASE
                    WHEN agent_runs.lineage_kind = 'subagent_child'
                    THEN agent_runs.run_id
                    ELSE NULL
                END,
                COALESCE(agent_runs.subagent_role, agent_runs.runtime_agent_id),
                agent_runs.trace_id,
                ?4,
                ?5,
                NULL,
                ?6,
                ?7,
                ?8,
                ?9,
                'open',
                ?10,
                ?11
            FROM agent_runs
            WHERE agent_runs.project_id = ?2
              AND agent_runs.run_id = ?12
            "#,
            params![
                item_id.as_str(),
                notice.project_id,
                notice.item_type.as_str(),
                notice.target_agent_session_id,
                notice.target_run_id,
                notice.title.as_str(),
                notice.body.as_str(),
                related_paths_json.as_str(),
                notice.priority.as_str(),
                notice.created_at,
                expires_at.as_str(),
                notice.sender_run_id,
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_notice_insert_failed",
                format!(
                    "Xero could not publish code history mailbox notice for operation `{}` in {}: {error}",
                    notice.operation_id,
                    database_path_for_repo(repo_root).display()
                ),
            )
        })?;
    if inserted == 0 {
        return Ok(());
    }
    Ok(())
}

fn code_history_mailbox_notice_exists(
    connection: &Connection,
    project_id: &str,
    item_id: &str,
) -> CommandResult<bool> {
    let count = connection
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM agent_mailbox_items
            WHERE project_id = ?1
              AND item_id = ?2
            "#,
            params![project_id, item_id],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_mailbox_notice_lookup_failed",
                format!("Xero could not check existing code history mailbox notice: {error}"),
            )
        })?;
    Ok(count > 0)
}

fn code_history_overlap_mailbox_item_type(
    status: CodeHistoryOperationStatus,
) -> Option<AgentMailboxItemType> {
    match status {
        CodeHistoryOperationStatus::Completed | CodeHistoryOperationStatus::RepairNeeded => {
            Some(AgentMailboxItemType::HistoryRewriteNotice)
        }
        CodeHistoryOperationStatus::Conflicted => Some(AgentMailboxItemType::UndoConflictNotice),
        CodeHistoryOperationStatus::Failed
        | CodeHistoryOperationStatus::Pending
        | CodeHistoryOperationStatus::Planning
        | CodeHistoryOperationStatus::Applying => None,
    }
}

fn code_history_should_publish_project_mailbox_notice(status: CodeHistoryOperationStatus) -> bool {
    matches!(
        status,
        CodeHistoryOperationStatus::Completed | CodeHistoryOperationStatus::RepairNeeded
    )
}

fn code_history_should_invalidate_reservations(status: CodeHistoryOperationStatus) -> bool {
    matches!(
        status,
        CodeHistoryOperationStatus::Completed | CodeHistoryOperationStatus::RepairNeeded
    )
}

fn code_history_mailbox_title(
    mode: &str,
    status: CodeHistoryOperationStatus,
    priority: AgentMailboxPriority,
) -> String {
    let mode = code_history_operation_mode_label(mode);
    match (status, priority) {
        (CodeHistoryOperationStatus::Completed, AgentMailboxPriority::High) => {
            format!("{mode} changed reserved or recently active paths")
        }
        (CodeHistoryOperationStatus::Completed, _) => {
            format!("{mode} advanced the workspace")
        }
        (CodeHistoryOperationStatus::Conflicted, _) => {
            format!("{mode} conflicted before writing")
        }
        (CodeHistoryOperationStatus::RepairNeeded, AgentMailboxPriority::High) => {
            format!("{mode} needs repair on active paths")
        }
        (CodeHistoryOperationStatus::RepairNeeded, _) => {
            format!("{mode} needs workspace repair")
        }
        _ => format!("{mode} updated code history"),
    }
}

fn code_history_mailbox_body(
    row: &CodeHistoryCoordinationEventRow,
    status: CodeHistoryOperationStatus,
    affected_paths: &[String],
    conflict_count: usize,
    recipient: &CodeHistoryMailboxRecipient,
) -> String {
    let reasons = if recipient.reasons.is_empty() {
        "same-project history update".to_string()
    } else {
        recipient
            .reasons
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ")
    };
    let matching_paths = if recipient.matching_paths.is_empty() {
        format_path_preview(affected_paths)
    } else {
        format_path_preview(&recipient.matching_paths.iter().cloned().collect::<Vec<_>>())
    };
    let affected_paths = format_path_preview(affected_paths);
    let result_commit = row
        .result_commit_id
        .as_deref()
        .map(|commit_id| format!(" Result commit: `{commit_id}`."))
        .unwrap_or_default();
    let repair = row
        .repair_code
        .as_deref()
        .map(|code| format!(" Repair code: `{code}`."))
        .unwrap_or_default();
    let failure = row
        .failure_code
        .as_deref()
        .map(|code| format!(" Failure code: `{code}`."))
        .unwrap_or_default();

    match status {
        CodeHistoryOperationStatus::Completed => format!(
            "Code history operation `{}` completed. Notice reason: {}. Affected paths: {}. Matching paths for this run: {}. Re-read current files before overlapping writes.{}",
            row.operation_id, reasons, affected_paths, matching_paths, result_commit
        ),
        CodeHistoryOperationStatus::Conflicted => format!(
            "Code history operation `{}` reported {} {} before writing files. Notice reason: {}. Affected paths: {}. Matching paths for this run: {}. Check current files before continuing overlapping work.",
            row.operation_id,
            conflict_count,
            plural_noun("conflict", conflict_count),
            reasons,
            affected_paths,
            matching_paths
        ),
        CodeHistoryOperationStatus::RepairNeeded => format!(
            "Code history operation `{}` needs repair after an interrupted apply. Notice reason: {}. Affected paths: {}. Matching paths for this run: {}. Re-read current files before overlapping writes.{}{}",
            row.operation_id, reasons, affected_paths, matching_paths, result_commit, repair
        ),
        CodeHistoryOperationStatus::Failed => format!(
            "Code history operation `{}` failed before completion. Notice reason: {}. Affected paths: {}.{}",
            row.operation_id, reasons, affected_paths, failure
        ),
        CodeHistoryOperationStatus::Pending
        | CodeHistoryOperationStatus::Planning
        | CodeHistoryOperationStatus::Applying => format!(
            "Code history operation `{}` updated status `{}`. Notice reason: {}. Affected paths: {}.",
            row.operation_id,
            status.as_str(),
            reasons,
            affected_paths
        ),
    }
}

fn code_history_mailbox_item_id(
    operation_id: &str,
    item_type: &str,
    target_run_id: &str,
) -> String {
    let digest = sha256_hex(format!("{operation_id}\0{item_type}\0{target_run_id}").as_bytes());
    format!("history-mailbox-{}", &digest[..24])
}

fn mailbox_priority_rank(priority: AgentMailboxPriority) -> u8 {
    match priority {
        AgentMailboxPriority::Urgent => 0,
        AgentMailboxPriority::High => 1,
        AgentMailboxPriority::Normal => 2,
        AgentMailboxPriority::Low => 3,
    }
}

fn format_path_preview(paths: &[String]) -> String {
    if paths.is_empty() {
        return "none".into();
    }
    let mut preview = paths.iter().take(8).cloned().collect::<Vec<_>>();
    if paths.len() > preview.len() {
        preview.push(format!("and {} more", paths.len() - preview.len()));
    }
    preview.join(", ")
}

fn read_code_history_coordination_event_row(
    connection: &Connection,
    project_id: &str,
    operation_id: &str,
) -> CommandResult<Option<CodeHistoryCoordinationEventRow>> {
    connection
        .query_row(
            r#"
            SELECT
                operation_id,
                mode,
                target_kind,
                target_id,
                target_change_group_id,
                target_file_path,
                target_hunk_ids_json,
                agent_session_id,
                run_id,
                expected_workspace_epoch,
                affected_paths_json,
                conflicts_json,
                result_change_group_id,
                result_commit_id,
                failure_code,
                failure_message,
                repair_code,
                repair_message,
                updated_at,
                completed_at
            FROM code_history_operations
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![project_id, operation_id],
            |row| {
                Ok(CodeHistoryCoordinationEventRow {
                    operation_id: row.get(0)?,
                    mode: row.get(1)?,
                    target_kind: row.get(2)?,
                    target_id: row.get(3)?,
                    target_change_group_id: row.get(4)?,
                    target_file_path: row.get(5)?,
                    target_hunk_ids_json: row.get(6)?,
                    agent_session_id: row.get(7)?,
                    run_id: row.get(8)?,
                    expected_workspace_epoch: row.get(9)?,
                    affected_paths_json: row.get(10)?,
                    conflicts_json: row.get(11)?,
                    result_change_group_id: row.get(12)?,
                    result_commit_id: row.get(13)?,
                    failure_code: row.get(14)?,
                    failure_message: row.get(15)?,
                    repair_code: row.get(16)?,
                    repair_message: row.get(17)?,
                    updated_at: row.get(18)?,
                    completed_at: row.get(19)?,
                })
            },
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_coordination_operation_query_failed",
                format!(
                    "Xero could not read code history operation `{operation_id}` for coordination publishing: {error}"
                ),
            )
        })
}

fn code_history_coordination_event_already_published(
    connection: &Connection,
    project_id: &str,
    run_id: &str,
    event_kind: &str,
    operation_id: &str,
    status: &str,
) -> CommandResult<bool> {
    let count = connection
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM agent_coordination_events
            WHERE project_id = ?1
              AND run_id = ?2
              AND event_kind = ?3
              AND json_extract(payload_json, '$.operationId') = ?4
              AND json_extract(payload_json, '$.status') = ?5
            "#,
            params![project_id, run_id, event_kind, operation_id, status],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_coordination_event_lookup_failed",
                format!(
                    "Xero could not check existing code history coordination events for operation `{operation_id}`: {error}"
                ),
            )
        })?;
    Ok(count > 0)
}

fn decode_code_history_operation_json(raw: &str, field: &'static str) -> CommandResult<JsonValue> {
    serde_json::from_str(raw).map_err(|error| {
        CommandError::system_fault(
            "code_history_coordination_payload_decode_failed",
            format!("Xero could not decode code history operation {field} JSON: {error}"),
        )
    })
}

fn code_history_json_string_list(
    value: &JsonValue,
    field: &'static str,
) -> CommandResult<Vec<String>> {
    let Some(values) = value.as_array() else {
        return Err(CommandError::system_fault(
            "code_history_coordination_payload_decode_failed",
            format!("Code history operation {field} must be a JSON string array."),
        ));
    };
    let mut decoded = Vec::with_capacity(values.len());
    for value in values {
        let Some(value) = value.as_str() else {
            return Err(CommandError::system_fault(
                "code_history_coordination_payload_decode_failed",
                format!("Code history operation {field} must contain only strings."),
            ));
        };
        decoded.push(value.to_string());
    }
    Ok(decoded)
}

fn optional_code_history_issue_json(code: Option<String>, message: Option<String>) -> JsonValue {
    match (code, message) {
        (Some(code), Some(message)) => json!({
            "code": code,
            "message": message,
        }),
        _ => JsonValue::Null,
    }
}

fn code_history_coordination_event_kind(status: CodeHistoryOperationStatus) -> &'static str {
    match status {
        CodeHistoryOperationStatus::Conflicted => "undo_conflict_notice",
        CodeHistoryOperationStatus::Failed => "history_operation_failed",
        CodeHistoryOperationStatus::RepairNeeded => "history_operation_repair_needed",
        CodeHistoryOperationStatus::Completed
        | CodeHistoryOperationStatus::Pending
        | CodeHistoryOperationStatus::Planning
        | CodeHistoryOperationStatus::Applying => "history_rewrite_notice",
    }
}

fn code_history_coordination_summary(
    mode: &JsonValue,
    status: CodeHistoryOperationStatus,
    affected_path_count: usize,
    conflict_count: usize,
) -> String {
    let mode = mode
        .as_str()
        .map(code_history_operation_mode_label)
        .unwrap_or("Code history operation");
    match status {
        CodeHistoryOperationStatus::Completed => format!(
            "{mode} completed across {affected_path_count} {}.",
            plural_noun("path", affected_path_count)
        ),
        CodeHistoryOperationStatus::Conflicted => format!(
            "{mode} conflicted on {conflict_count} {} across {affected_path_count} {}.",
            plural_noun("conflict", conflict_count),
            plural_noun("path", affected_path_count)
        ),
        CodeHistoryOperationStatus::Failed => format!("{mode} failed before completion."),
        CodeHistoryOperationStatus::RepairNeeded => {
            format!("{mode} needs repair after an interrupted apply.")
        }
        CodeHistoryOperationStatus::Pending
        | CodeHistoryOperationStatus::Planning
        | CodeHistoryOperationStatus::Applying => {
            format!("{mode} updated code history coordination state.")
        }
    }
}

fn code_history_operation_mode_label(mode: &str) -> &'static str {
    match mode {
        "selective_undo" => "Selective undo",
        "session_rollback" => "Session rollback",
        _ => "Code history operation",
    }
}

fn plural_noun(noun: &'static str, count: usize) -> &'static str {
    if count == 1 {
        noun
    } else {
        match noun {
            "path" => "paths",
            "conflict" => "conflicts",
            _ => noun,
        }
    }
}

fn timestamp_plus_history_coordination_lease(timestamp: &str) -> CommandResult<String> {
    let timestamp = OffsetDateTime::parse(timestamp, &Rfc3339).map_err(|error| {
        CommandError::system_fault(
            "code_history_coordination_timestamp_invalid",
            format!("Xero could not parse code history coordination timestamp: {error}"),
        )
    })?;
    (timestamp + Duration::seconds(DEFAULT_HISTORY_COORDINATION_EVENT_LEASE_SECONDS))
        .format(&Rfc3339)
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_coordination_timestamp_format_failed",
                format!("Xero could not format code history coordination expiry: {error}"),
            )
        })
}

fn string_list_json(values: &[String]) -> CommandResult<String> {
    serde_json::to_string(values).map_err(|error| {
        CommandError::system_fault(
            "code_history_operation_json_encode_failed",
            format!("Xero could not encode code history operation list: {error}"),
        )
    })
}

fn code_history_conflicts_json(conflicts: &[CodeFileUndoConflict]) -> CommandResult<String> {
    let values = conflicts
        .iter()
        .map(|conflict| {
            json!({
                "path": conflict.path.as_str(),
                "kind": conflict.kind.as_str(),
                "message": conflict.message.as_str(),
                "baseHash": conflict.base_hash.as_deref(),
                "selectedHash": conflict.selected_hash.as_deref(),
                "currentHash": conflict.current_hash.as_deref(),
                "hunkIds": &conflict.hunk_ids,
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&values).map_err(|error| {
        CommandError::system_fault(
            "code_history_operation_json_encode_failed",
            format!("Xero could not encode code history operation conflicts: {error}"),
        )
    })
}

fn normalize_history_paths(paths: &[String]) -> CommandResult<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for path in paths {
        validate_non_empty(path, "affectedPath")?;
        normalized.insert(path.clone());
    }
    Ok(normalized.into_iter().collect())
}

fn u64_to_i64(value: u64, field: &'static str) -> CommandResult<i64> {
    i64::try_from(value).map_err(|_| CommandError::invalid_request(field))
}

#[derive(Debug)]
struct CodeRollbackOperationRow {
    project_id: String,
    operation_id: String,
    agent_session_id: String,
    run_id: String,
    target_change_group_id: String,
    target_snapshot_id: String,
    pre_rollback_snapshot_id: Option<String>,
    result_change_group_id: Option<String>,
    status: String,
    failure_code: Option<String>,
    failure_message: Option<String>,
    affected_files_json: String,
    target_summary_label: Option<String>,
    result_summary_label: Option<String>,
    created_at: String,
    completed_at: Option<String>,
}

#[derive(Debug)]
struct CodeHistoryOperationRow {
    project_id: String,
    operation_id: String,
    mode: String,
    status: String,
    target_kind: String,
    target_id: String,
    target_change_group_id: Option<String>,
    target_file_path: Option<String>,
    target_hunk_ids_json: String,
    agent_session_id: String,
    run_id: String,
    expected_workspace_epoch: Option<i64>,
    affected_paths_json: String,
    conflicts_json: String,
    result_change_group_id: Option<String>,
    result_commit_id: Option<String>,
    failure_code: Option<String>,
    failure_message: Option<String>,
    repair_code: Option<String>,
    repair_message: Option<String>,
    target_summary_label: Option<String>,
    result_summary_label: Option<String>,
    created_at: String,
    updated_at: String,
    completed_at: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodeRollbackAffectedFileWire {
    path_before: Option<String>,
    path_after: Option<String>,
    operation: String,
    before_hash: Option<String>,
    after_hash: Option<String>,
    explicitly_edited: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CodeHistoryOperationConflictWire {
    path: String,
    kind: String,
    message: String,
    base_hash: Option<String>,
    selected_hash: Option<String>,
    current_hash: Option<String>,
    #[serde(default)]
    hunk_ids: Vec<String>,
}

fn query_code_rollback_operations_for_session(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<Vec<CodeRollbackOperationRecord>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                operation.project_id,
                operation.operation_id,
                operation.agent_session_id,
                operation.run_id,
                operation.target_change_group_id,
                operation.target_snapshot_id,
                operation.pre_rollback_snapshot_id,
                operation.result_change_group_id,
                operation.status,
                operation.failure_code,
                operation.failure_message,
                operation.affected_files_json,
                target.summary_label AS target_summary_label,
                result.summary_label AS result_summary_label,
                operation.created_at,
                operation.completed_at
            FROM code_rollback_operations operation
            LEFT JOIN code_change_groups target
              ON target.project_id = operation.project_id
             AND target.change_group_id = operation.target_change_group_id
            LEFT JOIN code_change_groups result
              ON result.project_id = operation.project_id
             AND result.change_group_id = operation.result_change_group_id
            WHERE operation.project_id = ?1
              AND operation.agent_session_id = ?2
            ORDER BY operation.created_at ASC, operation.operation_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!(
                    "Xero could not prepare code rollback operation query in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let mapped = statement
        .query_map(params![project_id, agent_session_id], |row| {
            read_code_rollback_operation_row(row)
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!(
                    "Xero could not query code rollback operations in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    collect_code_rollback_operation_rows(mapped, database_path)
}

fn query_code_rollback_operations_for_run(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
) -> CommandResult<Vec<CodeRollbackOperationRecord>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                operation.project_id,
                operation.operation_id,
                operation.agent_session_id,
                operation.run_id,
                operation.target_change_group_id,
                operation.target_snapshot_id,
                operation.pre_rollback_snapshot_id,
                operation.result_change_group_id,
                operation.status,
                operation.failure_code,
                operation.failure_message,
                operation.affected_files_json,
                target.summary_label AS target_summary_label,
                result.summary_label AS result_summary_label,
                operation.created_at,
                operation.completed_at
            FROM code_rollback_operations operation
            LEFT JOIN code_change_groups target
              ON target.project_id = operation.project_id
             AND target.change_group_id = operation.target_change_group_id
            LEFT JOIN code_change_groups result
              ON result.project_id = operation.project_id
             AND result.change_group_id = operation.result_change_group_id
            WHERE operation.project_id = ?1
              AND operation.agent_session_id = ?2
              AND operation.run_id = ?3
            ORDER BY operation.created_at ASC, operation.operation_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!(
                    "Xero could not prepare run code rollback operation query in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let mapped = statement
        .query_map(params![project_id, agent_session_id, run_id], |row| {
            read_code_rollback_operation_row(row)
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!(
                    "Xero could not query run code rollback operations in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    collect_code_rollback_operation_rows(mapped, database_path)
}

fn query_code_history_operations_for_session(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
) -> CommandResult<Vec<CodeHistoryOperationRecord>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                operation.project_id,
                operation.operation_id,
                operation.mode,
                operation.status,
                operation.target_kind,
                operation.target_id,
                operation.target_change_group_id,
                operation.target_file_path,
                operation.target_hunk_ids_json,
                operation.agent_session_id,
                operation.run_id,
                operation.expected_workspace_epoch,
                operation.affected_paths_json,
                operation.conflicts_json,
                operation.result_change_group_id,
                operation.result_commit_id,
                operation.failure_code,
                operation.failure_message,
                operation.repair_code,
                operation.repair_message,
                target.summary_label AS target_summary_label,
                result.summary_label AS result_summary_label,
                operation.created_at,
                operation.updated_at,
                operation.completed_at
            FROM code_history_operations operation
            LEFT JOIN code_change_groups target
              ON target.project_id = operation.project_id
             AND target.change_group_id = operation.target_change_group_id
            LEFT JOIN code_change_groups result
              ON result.project_id = operation.project_id
             AND result.change_group_id = operation.result_change_group_id
            WHERE operation.project_id = ?1
              AND operation.agent_session_id = ?2
              AND operation.run_id IS NOT NULL
            ORDER BY COALESCE(operation.completed_at, operation.updated_at, operation.created_at) ASC,
                     operation.operation_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!(
                    "Xero could not prepare code history operation query in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let mapped = statement
        .query_map(params![project_id, agent_session_id], |row| {
            read_code_history_operation_row(row)
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!(
                    "Xero could not query code history operations in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    collect_code_history_operation_rows(mapped, database_path)
}

fn query_code_history_operations_for_run(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    agent_session_id: &str,
    run_id: &str,
) -> CommandResult<Vec<CodeHistoryOperationRecord>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT
                operation.project_id,
                operation.operation_id,
                operation.mode,
                operation.status,
                operation.target_kind,
                operation.target_id,
                operation.target_change_group_id,
                operation.target_file_path,
                operation.target_hunk_ids_json,
                operation.agent_session_id,
                operation.run_id,
                operation.expected_workspace_epoch,
                operation.affected_paths_json,
                operation.conflicts_json,
                operation.result_change_group_id,
                operation.result_commit_id,
                operation.failure_code,
                operation.failure_message,
                operation.repair_code,
                operation.repair_message,
                target.summary_label AS target_summary_label,
                result.summary_label AS result_summary_label,
                operation.created_at,
                operation.updated_at,
                operation.completed_at
            FROM code_history_operations operation
            LEFT JOIN code_change_groups target
              ON target.project_id = operation.project_id
             AND target.change_group_id = operation.target_change_group_id
            LEFT JOIN code_change_groups result
              ON result.project_id = operation.project_id
             AND result.change_group_id = operation.result_change_group_id
            WHERE operation.project_id = ?1
              AND operation.agent_session_id = ?2
              AND operation.run_id = ?3
            ORDER BY COALESCE(operation.completed_at, operation.updated_at, operation.created_at) ASC,
                     operation.operation_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!(
                    "Xero could not prepare run code history operation query in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let mapped = statement
        .query_map(params![project_id, agent_session_id, run_id], |row| {
            read_code_history_operation_row(row)
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!(
                    "Xero could not query run code history operations in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    collect_code_history_operation_rows(mapped, database_path)
}

fn read_code_rollback_operation_row(row: &Row<'_>) -> rusqlite::Result<CodeRollbackOperationRow> {
    Ok(CodeRollbackOperationRow {
        project_id: row.get(0)?,
        operation_id: row.get(1)?,
        agent_session_id: row.get(2)?,
        run_id: row.get(3)?,
        target_change_group_id: row.get(4)?,
        target_snapshot_id: row.get(5)?,
        pre_rollback_snapshot_id: row.get(6)?,
        result_change_group_id: row.get(7)?,
        status: row.get(8)?,
        failure_code: row.get(9)?,
        failure_message: row.get(10)?,
        affected_files_json: row.get(11)?,
        target_summary_label: row.get(12)?,
        result_summary_label: row.get(13)?,
        created_at: row.get(14)?,
        completed_at: row.get(15)?,
    })
}

fn collect_code_rollback_operation_rows<F>(
    rows: rusqlite::MappedRows<'_, F>,
    database_path: &Path,
) -> CommandResult<Vec<CodeRollbackOperationRecord>>
where
    F: FnMut(&Row<'_>) -> rusqlite::Result<CodeRollbackOperationRow>,
{
    let mut operations = Vec::new();
    for row in rows {
        let row = row.map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!(
                    "Xero could not read a code rollback operation row from {}: {error}",
                    database_path.display()
                ),
            )
        })?;
        operations.push(code_rollback_operation_record_from_row(row, database_path)?);
    }
    Ok(operations)
}

fn code_rollback_operation_record_from_row(
    row: CodeRollbackOperationRow,
    database_path: &Path,
) -> CommandResult<CodeRollbackOperationRecord> {
    let affected_files = decode_code_rollback_affected_files(
        row.operation_id.as_str(),
        row.affected_files_json.as_str(),
        database_path,
    )?;
    Ok(CodeRollbackOperationRecord {
        project_id: row.project_id,
        operation_id: row.operation_id,
        agent_session_id: row.agent_session_id,
        run_id: row.run_id,
        target_change_group_id: row.target_change_group_id,
        target_snapshot_id: row.target_snapshot_id,
        pre_rollback_snapshot_id: row.pre_rollback_snapshot_id,
        result_change_group_id: row.result_change_group_id,
        status: row.status,
        failure_code: row.failure_code,
        failure_message: row.failure_message,
        affected_files,
        target_summary_label: row.target_summary_label,
        result_summary_label: row.result_summary_label,
        created_at: row.created_at,
        completed_at: row.completed_at,
    })
}

fn read_code_history_operation_row(row: &Row<'_>) -> rusqlite::Result<CodeHistoryOperationRow> {
    Ok(CodeHistoryOperationRow {
        project_id: row.get(0)?,
        operation_id: row.get(1)?,
        mode: row.get(2)?,
        status: row.get(3)?,
        target_kind: row.get(4)?,
        target_id: row.get(5)?,
        target_change_group_id: row.get(6)?,
        target_file_path: row.get(7)?,
        target_hunk_ids_json: row.get(8)?,
        agent_session_id: row.get(9)?,
        run_id: row.get(10)?,
        expected_workspace_epoch: row.get(11)?,
        affected_paths_json: row.get(12)?,
        conflicts_json: row.get(13)?,
        result_change_group_id: row.get(14)?,
        result_commit_id: row.get(15)?,
        failure_code: row.get(16)?,
        failure_message: row.get(17)?,
        repair_code: row.get(18)?,
        repair_message: row.get(19)?,
        target_summary_label: row.get(20)?,
        result_summary_label: row.get(21)?,
        created_at: row.get(22)?,
        updated_at: row.get(23)?,
        completed_at: row.get(24)?,
    })
}

fn collect_code_history_operation_rows<F>(
    rows: rusqlite::MappedRows<'_, F>,
    database_path: &Path,
) -> CommandResult<Vec<CodeHistoryOperationRecord>>
where
    F: FnMut(&Row<'_>) -> rusqlite::Result<CodeHistoryOperationRow>,
{
    let mut operations = Vec::new();
    for row in rows {
        let row = row.map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!(
                    "Xero could not read a code history operation row from {}: {error}",
                    database_path.display()
                ),
            )
        })?;
        operations.push(code_history_operation_record_from_row(row, database_path)?);
    }
    Ok(operations)
}

fn code_history_operation_record_from_row(
    row: CodeHistoryOperationRow,
    database_path: &Path,
) -> CommandResult<CodeHistoryOperationRecord> {
    let target_hunk_ids = decode_code_history_string_list(
        row.operation_id.as_str(),
        "targetHunkIds",
        row.target_hunk_ids_json.as_str(),
        database_path,
    )?;
    let affected_paths = decode_code_history_string_list(
        row.operation_id.as_str(),
        "affectedPaths",
        row.affected_paths_json.as_str(),
        database_path,
    )?;
    let conflicts = decode_code_history_conflicts(
        row.operation_id.as_str(),
        row.conflicts_json.as_str(),
        database_path,
    )?;
    let expected_workspace_epoch = row
        .expected_workspace_epoch
        .map(|epoch| {
            u64::try_from(epoch).map_err(|_| {
                CommandError::system_fault(
                    "code_history_operation_query_failed",
                    format!(
                        "Code history operation `{}` has invalid expected workspace epoch `{epoch}` in {}.",
                        row.operation_id,
                        database_path.display()
                    ),
                )
            })
        })
        .transpose()?;

    Ok(CodeHistoryOperationRecord {
        project_id: row.project_id,
        operation_id: row.operation_id,
        mode: row.mode,
        status: row.status,
        target_kind: row.target_kind,
        target_id: row.target_id,
        target_change_group_id: row.target_change_group_id,
        target_file_path: row.target_file_path,
        target_hunk_ids,
        agent_session_id: row.agent_session_id,
        run_id: row.run_id,
        expected_workspace_epoch,
        affected_paths,
        conflicts,
        result_change_group_id: row.result_change_group_id,
        result_commit_id: row.result_commit_id,
        failure_code: row.failure_code,
        failure_message: row.failure_message,
        repair_code: row.repair_code,
        repair_message: row.repair_message,
        target_summary_label: row.target_summary_label,
        result_summary_label: row.result_summary_label,
        created_at: row.created_at,
        updated_at: row.updated_at,
        completed_at: row.completed_at,
    })
}

fn decode_code_rollback_affected_files(
    operation_id: &str,
    affected_files_json: &str,
    database_path: &Path,
) -> CommandResult<Vec<CompletedCodeChangeFile>> {
    let wires = serde_json::from_str::<Vec<CodeRollbackAffectedFileWire>>(affected_files_json)
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_affected_files_decode_failed",
                format!(
                    "Xero could not decode affected files for rollback operation `{operation_id}` from {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    wires
        .into_iter()
        .map(|wire| {
            Ok(CompletedCodeChangeFile {
                path_before: wire.path_before,
                path_after: wire.path_after,
                operation: parse_code_file_operation(&wire.operation).ok_or_else(|| {
                    CommandError::system_fault(
                        "code_rollback_affected_file_operation_invalid",
                        format!(
                            "Rollback operation `{operation_id}` recorded unsupported file operation `{}`.",
                            wire.operation
                        ),
                    )
                })?,
                before_hash: wire.before_hash,
                after_hash: wire.after_hash,
                explicitly_edited: wire.explicitly_edited,
            })
        })
        .collect()
}

fn decode_code_history_string_list(
    operation_id: &str,
    field: &'static str,
    raw_json: &str,
    database_path: &Path,
) -> CommandResult<Vec<String>> {
    serde_json::from_str::<Vec<String>>(raw_json).map_err(|error| {
        CommandError::system_fault(
            "code_history_operation_query_failed",
            format!(
                "Xero could not decode {field} for code history operation `{operation_id}` from {}: {error}",
                database_path.display()
            ),
        )
    })
}

fn decode_code_history_conflicts(
    operation_id: &str,
    raw_json: &str,
    database_path: &Path,
) -> CommandResult<Vec<CodeHistoryOperationConflictRecord>> {
    let wires = serde_json::from_str::<Vec<CodeHistoryOperationConflictWire>>(raw_json).map_err(
        |error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!(
                    "Xero could not decode conflicts for code history operation `{operation_id}` from {}: {error}",
                    database_path.display()
                ),
            )
        },
    )?;
    Ok(wires
        .into_iter()
        .map(|wire| CodeHistoryOperationConflictRecord {
            path: wire.path,
            kind: wire.kind,
            message: wire.message,
            base_hash: wire.base_hash,
            selected_hash: wire.selected_hash,
            current_hash: wire.current_hash,
            hunk_ids: wire.hunk_ids,
        })
        .collect())
}

fn parse_code_file_operation(value: &str) -> Option<CodeFileOperation> {
    match value {
        "create" => Some(CodeFileOperation::Create),
        "modify" => Some(CodeFileOperation::Modify),
        "delete" => Some(CodeFileOperation::Delete),
        "rename" => Some(CodeFileOperation::Rename),
        "mode_change" => Some(CodeFileOperation::ModeChange),
        "symlink_change" => Some(CodeFileOperation::SymlinkChange),
        _ => None,
    }
}

fn scan_project_manifest(
    repo_root: &Path,
    connection: &Connection,
    project_id: &str,
    previous_manifest: Option<&CodeSnapshotManifest>,
    explicit_paths: &BTreeSet<String>,
    scan_budget: CodeSnapshotScanBudget,
) -> CommandResult<CodeSnapshotManifest> {
    let previous_entries = previous_manifest.map(CodeSnapshotManifest::entry_map);
    let mut entries = BTreeMap::new();
    let mut visited_paths = BTreeSet::new();
    let mut forced_paths = BTreeSet::new();
    let mut scan_progress = CodeSnapshotScanProgress::default();
    let mut scan_truncated = false;

    let mut builder = WalkBuilder::new(repo_root);
    builder
        .follow_links(false)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .filter_entry(should_include_walk_entry);

    for entry in builder.build() {
        let entry = entry.map_err(|error| {
            CommandError::retryable(
                "code_snapshot_walk_failed",
                format!("Xero could not scan project files for code snapshot: {error}"),
            )
        })?;
        if entry.depth() == 0 {
            continue;
        }
        if !scan_progress.try_visit_walk_entry(scan_budget) {
            scan_truncated = true;
            break;
        }
        let relative_path = repo_relative_path(repo_root, entry.path())?;
        visited_paths.insert(relative_path.clone());
        if let Some(snapshot_entry) = capture_manifest_entry_with_budget(
            repo_root,
            connection,
            project_id,
            &relative_path,
            previous_entries.as_ref(),
            true,
            Some(&mut scan_progress),
            scan_budget,
            false,
        )? {
            entries.insert(relative_path, snapshot_entry);
        }
    }

    for explicit_path in explicit_paths {
        add_explicit_manifest_path(
            repo_root,
            connection,
            project_id,
            explicit_path,
            previous_entries.as_ref(),
            &mut entries,
            &mut forced_paths,
        )?;
    }

    if let Some(previous_entries) = previous_entries.as_ref() {
        for (path, entry) in previous_entries {
            if (scan_truncated || is_generated_or_ignored_path(path))
                && !entries.contains_key(path)
                && !visited_paths.contains(path)
                && !forced_paths.contains(path)
            {
                entries.insert(path.clone(), entry.clone());
            }
        }
    }

    Ok(CodeSnapshotManifest::new(
        repo_root,
        now_timestamp(),
        entries.into_values().collect(),
    ))
}

fn add_explicit_manifest_path(
    repo_root: &Path,
    connection: &Connection,
    project_id: &str,
    explicit_path: &str,
    previous_entries: Option<&BTreeMap<String, CodeSnapshotFileEntry>>,
    entries: &mut BTreeMap<String, CodeSnapshotFileEntry>,
    forced_paths: &mut BTreeSet<String>,
) -> CommandResult<()> {
    let Some(relative) = safe_relative_path(explicit_path) else {
        return Err(CommandError::invalid_request("explicitPath"));
    };

    let mut parent = PathBuf::new();
    if let Some(parent_path) = relative.parent() {
        for component in parent_path.components() {
            if let Component::Normal(segment) = component {
                parent.push(segment);
                let parent_key = path_to_forward_slash(&parent);
                forced_paths.insert(parent_key.clone());
                if let Some(entry) = capture_manifest_entry(
                    repo_root,
                    connection,
                    project_id,
                    &parent_key,
                    previous_entries,
                    true,
                )? {
                    entries.insert(parent_key, entry);
                }
            }
        }
    }

    let path_key = path_to_forward_slash(&relative);
    forced_paths.insert(path_key.clone());
    if let Some(entry) = capture_manifest_entry(
        repo_root,
        connection,
        project_id,
        &path_key,
        previous_entries,
        true,
    )? {
        entries.insert(path_key, entry);
    }
    Ok(())
}

fn capture_manifest_entry(
    repo_root: &Path,
    connection: &Connection,
    project_id: &str,
    relative_path: &str,
    previous_entries: Option<&BTreeMap<String, CodeSnapshotFileEntry>>,
    store_blobs: bool,
) -> CommandResult<Option<CodeSnapshotFileEntry>> {
    capture_manifest_entry_with_budget(
        repo_root,
        connection,
        project_id,
        relative_path,
        previous_entries,
        store_blobs,
        None,
        CodeSnapshotScanBudget::default(),
        true,
    )
}

fn capture_manifest_entry_with_budget(
    repo_root: &Path,
    connection: &Connection,
    project_id: &str,
    relative_path: &str,
    previous_entries: Option<&BTreeMap<String, CodeSnapshotFileEntry>>,
    store_blobs: bool,
    mut scan_progress: Option<&mut CodeSnapshotScanProgress>,
    scan_budget: CodeSnapshotScanBudget,
    force_capture: bool,
) -> CommandResult<Option<CodeSnapshotFileEntry>> {
    let Some(safe_relative) = safe_relative_path(relative_path) else {
        return Err(CommandError::invalid_request("path"));
    };
    let absolute_path = repo_root.join(&safe_relative);
    let metadata = match fs::symlink_metadata(&absolute_path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(CommandError::retryable(
                "code_snapshot_metadata_failed",
                format!(
                    "Xero could not inspect {} for code snapshot: {error}",
                    absolute_path.display()
                ),
            ));
        }
    };
    let mode = file_mode(&metadata);
    let modified_at_millis = modified_at_millis(&metadata);
    let path_key = path_to_forward_slash(&safe_relative);
    let file_type = metadata.file_type();

    if file_type.is_dir() {
        return Ok(Some(CodeSnapshotFileEntry {
            path: path_key,
            kind: CodeSnapshotFileKind::Directory,
            size: None,
            sha256: None,
            blob_id: None,
            mode,
            modified_at_millis,
            symlink_target: None,
        }));
    }

    if file_type.is_symlink() {
        let target = fs::read_link(&absolute_path).map_err(|error| {
            CommandError::retryable(
                "code_snapshot_symlink_read_failed",
                format!(
                    "Xero could not read symlink target for {}: {error}",
                    absolute_path.display()
                ),
            )
        })?;
        return Ok(Some(CodeSnapshotFileEntry {
            path: path_key,
            kind: CodeSnapshotFileKind::Symlink,
            size: Some(metadata.len()),
            sha256: None,
            blob_id: None,
            mode,
            modified_at_millis,
            symlink_target: Some(target.to_string_lossy().into_owned()),
        }));
    }

    if !file_type.is_file() {
        return Ok(None);
    }

    let size = metadata.len();
    if let Some(previous) = previous_entries.and_then(|entries| entries.get(&path_key)) {
        if previous.kind == CodeSnapshotFileKind::File
            && previous.size == Some(size)
            && previous.mode == mode
            && previous.modified_at_millis == modified_at_millis
            && previous.sha256.is_some()
            && previous.blob_id.is_some()
        {
            return Ok(Some(previous.clone()));
        }
    }

    if !store_blobs {
        return Ok(Some(CodeSnapshotFileEntry {
            path: path_key,
            kind: CodeSnapshotFileKind::File,
            size: Some(size),
            sha256: None,
            blob_id: None,
            mode,
            modified_at_millis,
            symlink_target: None,
        }));
    }

    if !force_capture {
        if let Some(progress) = scan_progress.as_deref_mut() {
            if !progress.try_reserve_new_blob_bytes(scan_budget, size) {
                return Ok(previous_entries
                    .and_then(|entries| entries.get(&path_key))
                    .cloned());
            }
        }
    }

    let bytes = fs::read(&absolute_path).map_err(|error| {
        CommandError::retryable(
            "code_snapshot_file_read_failed",
            format!(
                "Xero could not read {} for code snapshot: {error}",
                absolute_path.display()
            ),
        )
    })?;
    let blob_id = ensure_blob(connection, repo_root, project_id, &bytes)?;
    Ok(Some(CodeSnapshotFileEntry {
        path: path_key,
        kind: CodeSnapshotFileKind::File,
        size: Some(size),
        sha256: Some(blob_id.clone()),
        blob_id: Some(blob_id),
        mode,
        modified_at_millis,
        symlink_target: None,
    }))
}

fn ensure_blob(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    bytes: &[u8],
) -> CommandResult<String> {
    let blob_id = sha256_hex(bytes);
    let relative_path = blob_relative_path(&blob_id);
    let app_data_dir = project_app_data_dir_for_repo(repo_root);
    let absolute_path = app_data_dir.join(&relative_path);

    if absolute_path.exists() {
        let existing = fs::read(&absolute_path).map_err(|error| {
            CommandError::retryable(
                "code_blob_read_failed",
                format!(
                    "Xero could not read existing code blob {}: {error}",
                    absolute_path.display()
                ),
            )
        })?;
        if sha256_hex(&existing) != blob_id {
            write_blob_file(&absolute_path, bytes)?;
        }
    } else {
        write_blob_file(&absolute_path, bytes)?;
    }

    connection
        .execute(
            r#"
            INSERT INTO code_blobs (
                project_id,
                blob_id,
                sha256,
                size_bytes,
                storage_path,
                compression,
                created_at
            )
            VALUES (?1, ?2, ?2, ?3, ?4, 'none', ?5)
            ON CONFLICT(project_id, blob_id) DO UPDATE SET
                size_bytes = excluded.size_bytes,
                storage_path = excluded.storage_path,
                compression = excluded.compression
            "#,
            params![
                project_id,
                blob_id,
                saturating_usize_to_i64(bytes.len()),
                path_to_forward_slash(&relative_path),
                now_timestamp(),
            ],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_persist_failed",
                format!("Xero could not persist code blob `{blob_id}` metadata: {error}"),
            )
        })?;

    Ok(blob_id)
}

fn write_blob_file(path: &Path, bytes: &[u8]) -> CommandResult<()> {
    let parent = path.parent().ok_or_else(|| {
        CommandError::system_fault(
            "code_blob_path_invalid",
            format!("Xero resolved an invalid blob path {}.", path.display()),
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| {
        CommandError::retryable(
            "code_blob_dir_failed",
            format!(
                "Xero could not prepare code blob directory {}: {error}",
                parent.display()
            ),
        )
    })?;
    let temp_path = parent.join(format!(".{}.tmp", generate_id("code-blob")));
    fs::write(&temp_path, bytes).map_err(|error| {
        CommandError::retryable(
            "code_blob_write_failed",
            format!(
                "Xero could not write code blob {}: {error}",
                temp_path.display()
            ),
        )
    })?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        CommandError::retryable(
            "code_blob_rename_failed",
            format!(
                "Xero could not finalize code blob {}: {error}",
                path.display()
            ),
        )
    })?;
    Ok(())
}

fn load_completed_snapshot_manifest(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
    snapshot_id: &str,
) -> CommandResult<CodeSnapshotManifest> {
    let (write_state, manifest_json): (String, String) = connection
        .query_row(
            r#"
            SELECT write_state, manifest_json
            FROM code_snapshots
            WHERE project_id = ?1
              AND snapshot_id = ?2
            "#,
            params![project_id, snapshot_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "code_snapshot_query_failed",
                format!(
                    "Xero could not query code snapshot `{snapshot_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?
        .ok_or_else(|| {
            CommandError::user_fixable(
                "code_snapshot_missing",
                format!("Xero could not find code snapshot `{snapshot_id}`."),
            )
        })?;
    if write_state != "completed" {
        return Err(CommandError::retryable(
            "code_snapshot_unavailable",
            format!("Code snapshot `{snapshot_id}` is `{write_state}` and cannot be restored."),
        ));
    }
    serde_json::from_str::<CodeSnapshotManifest>(&manifest_json).map_err(|error| {
        CommandError::system_fault(
            "code_snapshot_manifest_decode_failed",
            format!(
                "Xero could not decode code snapshot `{snapshot_id}` from {}: {error}",
                database_path.display()
            ),
        )
    })
}

fn load_required_blob_bytes(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    manifest: &CodeSnapshotManifest,
) -> CommandResult<BTreeMap<String, Vec<u8>>> {
    let mut blobs = BTreeMap::new();
    for entry in &manifest.entries {
        if entry.kind != CodeSnapshotFileKind::File {
            continue;
        }
        let blob_id = entry.blob_id.as_deref().ok_or_else(|| {
            CommandError::system_fault(
                "code_snapshot_manifest_invalid",
                format!("Snapshot entry `{}` is missing its blob id.", entry.path),
            )
        })?;
        if blobs.contains_key(blob_id) {
            continue;
        }
        let bytes = read_blob_bytes(connection, repo_root, project_id, blob_id)?;
        blobs.insert(blob_id.into(), bytes);
    }
    Ok(blobs)
}

fn read_blob_bytes(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    blob_id: &str,
) -> CommandResult<Vec<u8>> {
    let storage_path: String = connection
        .query_row(
            r#"
            SELECT storage_path
            FROM code_blobs
            WHERE project_id = ?1
              AND blob_id = ?2
            "#,
            params![project_id, blob_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_query_failed",
                format!("Xero could not query code blob `{blob_id}`: {error}"),
            )
        })?
        .ok_or_else(|| {
            CommandError::retryable(
                "code_rollback_blob_missing",
                format!("Code snapshot requires blob `{blob_id}`, but its metadata is missing."),
            )
        })?;

    let Some(relative_path) = safe_relative_path(&storage_path) else {
        return Err(CommandError::system_fault(
            "code_blob_path_invalid",
            format!("Code blob `{blob_id}` has invalid storage path `{storage_path}`."),
        ));
    };
    let absolute_path = project_app_data_dir_for_repo(repo_root).join(relative_path);
    let bytes = fs::read(&absolute_path).map_err(|error| {
        CommandError::retryable(
            "code_rollback_blob_missing",
            format!(
                "Code snapshot requires blob `{blob_id}`, but {} could not be read: {error}",
                absolute_path.display()
            ),
        )
    })?;
    let observed = sha256_hex(&bytes);
    if observed != blob_id {
        return Err(CommandError::retryable(
            "code_rollback_blob_corrupt",
            format!(
                "Code blob `{blob_id}` at {} has hash `{observed}`.",
                absolute_path.display()
            ),
        ));
    }
    Ok(bytes)
}

#[cfg(test)]
fn apply_manifest_to_project(
    repo_root: &Path,
    snapshot_id: &str,
    manifest: &CodeSnapshotManifest,
    blob_bytes: &BTreeMap<String, Vec<u8>>,
) -> CommandResult<CodeSnapshotRestoreOutcome> {
    let target_entries = manifest.entry_map();
    let current_entries = scan_project_manifest_without_blobs(repo_root)?;
    let mut removed_paths = Vec::new();
    let mut restored_paths = Vec::new();

    let mut extra_paths = current_entries
        .keys()
        .filter(|path| !target_entries.contains_key(*path))
        .cloned()
        .collect::<Vec<_>>();
    extra_paths.sort_by(|left, right| {
        path_depth(right)
            .cmp(&path_depth(left))
            .then_with(|| right.cmp(left))
    });
    for path in extra_paths {
        remove_project_path(repo_root, &path)?;
        removed_paths.push(path);
    }

    let mut directories = target_entries
        .values()
        .filter(|entry| entry.kind == CodeSnapshotFileKind::Directory)
        .cloned()
        .collect::<Vec<_>>();
    directories.sort_by(|left, right| {
        path_depth(&left.path)
            .cmp(&path_depth(&right.path))
            .then_with(|| left.path.cmp(&right.path))
    });
    for entry in directories {
        ensure_directory_entry(repo_root, &entry)?;
        restored_paths.push(entry.path);
    }

    let mut symlinks = target_entries
        .values()
        .filter(|entry| entry.kind == CodeSnapshotFileKind::Symlink)
        .cloned()
        .collect::<Vec<_>>();
    symlinks.sort_by(|left, right| left.path.cmp(&right.path));
    for entry in symlinks {
        restore_symlink_entry(repo_root, &entry)?;
        restored_paths.push(entry.path);
    }

    let mut files = target_entries
        .values()
        .filter(|entry| entry.kind == CodeSnapshotFileKind::File)
        .cloned()
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.path.cmp(&right.path));
    for entry in files {
        restore_file_entry(repo_root, &entry, blob_bytes)?;
        restored_paths.push(entry.path);
    }

    Ok(CodeSnapshotRestoreOutcome {
        snapshot_id: snapshot_id.into(),
        restored_paths,
        removed_paths,
    })
}

#[cfg(test)]
fn scan_project_manifest_without_blobs(
    repo_root: &Path,
) -> CommandResult<BTreeMap<String, CodeSnapshotFileEntry>> {
    let mut entries = BTreeMap::new();
    let mut builder = WalkBuilder::new(repo_root);
    builder
        .follow_links(false)
        .hidden(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .filter_entry(should_include_walk_entry);

    let dummy_connection = Connection::open_in_memory().map_err(|error| {
        CommandError::system_fault(
            "code_snapshot_scan_failed",
            format!("Xero could not prepare in-memory scan state: {error}"),
        )
    })?;
    for entry in builder.build() {
        let entry = entry.map_err(|error| {
            CommandError::retryable(
                "code_snapshot_walk_failed",
                format!("Xero could not scan project files for restore: {error}"),
            )
        })?;
        if entry.depth() == 0 {
            continue;
        }
        let relative_path = repo_relative_path(repo_root, entry.path())?;
        if let Some(snapshot_entry) = capture_manifest_entry(
            repo_root,
            &dummy_connection,
            "",
            &relative_path,
            None,
            false,
        )? {
            entries.insert(relative_path, snapshot_entry);
        }
    }
    Ok(entries)
}

fn ensure_directory_entry(repo_root: &Path, entry: &CodeSnapshotFileEntry) -> CommandResult<()> {
    let path = absolute_repo_path(repo_root, &entry.path)?;
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
        Ok(_) => remove_path(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CommandError::retryable(
                "code_rollback_restore_inspect_failed",
                format!(
                    "Xero could not inspect {} before restore: {error}",
                    path.display()
                ),
            ));
        }
    }
    fs::create_dir_all(&path).map_err(|error| {
        CommandError::retryable(
            "code_rollback_restore_directory_failed",
            format!(
                "Xero could not restore directory {}: {error}",
                path.display()
            ),
        )
    })?;
    set_mode_if_supported(&path, entry.mode)?;
    Ok(())
}

fn restore_symlink_entry(repo_root: &Path, entry: &CodeSnapshotFileEntry) -> CommandResult<()> {
    let path = absolute_repo_path(repo_root, &entry.path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "code_rollback_restore_prepare_failed",
                format!(
                    "Xero could not prepare {} for restore: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    match fs::symlink_metadata(&path) {
        Ok(_) => remove_path(&path)?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(CommandError::retryable(
                "code_rollback_restore_inspect_failed",
                format!(
                    "Xero could not inspect {} before restore: {error}",
                    path.display()
                ),
            ));
        }
    }
    let target = entry.symlink_target.as_deref().ok_or_else(|| {
        CommandError::system_fault(
            "code_snapshot_manifest_invalid",
            format!(
                "Symlink snapshot entry `{}` is missing its target.",
                entry.path
            ),
        )
    })?;
    create_symlink(target, &path)?;
    Ok(())
}

fn restore_file_entry(
    repo_root: &Path,
    entry: &CodeSnapshotFileEntry,
    blob_bytes: &BTreeMap<String, Vec<u8>>,
) -> CommandResult<()> {
    let path = absolute_repo_path(repo_root, &entry.path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            CommandError::retryable(
                "code_rollback_restore_prepare_failed",
                format!(
                    "Xero could not prepare {} for restore: {error}",
                    parent.display()
                ),
            )
        })?;
    }
    match fs::symlink_metadata(&path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            remove_path(&path)?;
        }
        Ok(_) | Err(_) => {}
    }
    let blob_id = entry.blob_id.as_deref().ok_or_else(|| {
        CommandError::system_fault(
            "code_snapshot_manifest_invalid",
            format!(
                "File snapshot entry `{}` is missing its blob id.",
                entry.path
            ),
        )
    })?;
    let bytes = blob_bytes.get(blob_id).ok_or_else(|| {
        CommandError::retryable(
            "code_rollback_blob_missing",
            format!("Code snapshot requires blob `{blob_id}`, but it was not preloaded."),
        )
    })?;
    let parent = path.parent().unwrap_or(repo_root);
    let temp_path = parent.join(format!(".{}.tmp", generate_id("code-restore")));
    fs::write(&temp_path, bytes).map_err(|error| {
        CommandError::retryable(
            "code_rollback_restore_write_failed",
            format!(
                "Xero could not write restore file {}: {error}",
                temp_path.display()
            ),
        )
    })?;
    set_mode_if_supported(&temp_path, entry.mode)?;

    #[cfg(windows)]
    if path.exists() {
        let _ = fs::remove_file(&path);
    }

    fs::rename(&temp_path, &path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        CommandError::retryable(
            "code_rollback_restore_rename_failed",
            format!(
                "Xero could not finalize restore file {}: {error}",
                path.display()
            ),
        )
    })?;
    Ok(())
}

fn remove_project_path(repo_root: &Path, relative_path: &str) -> CommandResult<()> {
    let path = absolute_repo_path(repo_root, relative_path)?;
    remove_path(&path)
}

fn remove_path(path: &Path) -> CommandResult<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(CommandError::retryable(
                "code_rollback_remove_inspect_failed",
                format!(
                    "Xero could not inspect {} for removal: {error}",
                    path.display()
                ),
            ));
        }
    };
    if metadata.is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).map_err(|error| {
            CommandError::retryable(
                "code_rollback_remove_directory_failed",
                format!(
                    "Xero could not remove directory {}: {error}",
                    path.display()
                ),
            )
        })
    } else {
        fs::remove_file(path).map_err(|error| {
            CommandError::retryable(
                "code_rollback_remove_file_failed",
                format!("Xero could not remove file {}: {error}", path.display()),
            )
        })
    }
}

#[cfg(unix)]
fn create_symlink(target: &str, link_path: &Path) -> CommandResult<()> {
    std::os::unix::fs::symlink(target, link_path).map_err(|error| {
        CommandError::retryable(
            "code_rollback_restore_symlink_failed",
            format!(
                "Xero could not restore symlink {}: {error}",
                link_path.display()
            ),
        )
    })
}

#[cfg(windows)]
fn create_symlink(target: &str, link_path: &Path) -> CommandResult<()> {
    std::os::windows::fs::symlink_file(target, link_path).map_err(|error| {
        CommandError::retryable(
            "code_rollback_restore_symlink_failed",
            format!(
                "Xero could not restore symlink {}: {error}",
                link_path.display()
            ),
        )
    })
}

#[cfg(not(any(unix, windows)))]
fn create_symlink(_target: &str, link_path: &Path) -> CommandResult<()> {
    Err(CommandError::retryable(
        "code_rollback_restore_symlink_unsupported",
        format!(
            "Xero cannot restore symlink {} on this platform.",
            link_path.display()
        ),
    ))
}

fn open_code_rollback_database(repo_root: &Path) -> CommandResult<(Connection, PathBuf)> {
    let database_path = database_path_for_repo(repo_root);
    let connection = open_runtime_database(repo_root, &database_path)?;
    Ok((connection, database_path))
}

fn collect_snapshot_ids_by_state(
    connection: &Connection,
    project_id: &str,
    write_state: &str,
) -> CommandResult<Vec<String>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT snapshot_id
            FROM code_snapshots
            WHERE project_id = ?1
              AND write_state = ?2
            ORDER BY created_at ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_snapshot_query_failed",
                format!("Xero could not prepare code snapshot validation query: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id, write_state], |row| {
            row.get::<_, String>(0)
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_snapshot_query_failed",
                format!("Xero could not query code snapshots: {error}"),
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        CommandError::system_fault(
            "code_snapshot_query_failed",
            format!("Xero could not decode code snapshot rows: {error}"),
        )
    })
}

fn collect_completed_snapshot_rows(
    connection: &Connection,
    project_id: &str,
) -> CommandResult<Vec<(String, String)>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT snapshot_id, manifest_json
            FROM code_snapshots
            WHERE project_id = ?1
              AND write_state = 'completed'
            ORDER BY created_at ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_snapshot_query_failed",
                format!("Xero could not prepare code snapshot validation query: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| {
            CommandError::system_fault(
                "code_snapshot_query_failed",
                format!("Xero could not query code snapshots: {error}"),
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        CommandError::system_fault(
            "code_snapshot_query_failed",
            format!("Xero could not decode code snapshot rows: {error}"),
        )
    })
}

fn collect_pending_rollback_operations(
    connection: &Connection,
    project_id: &str,
) -> CommandResult<Vec<(String, Option<String>)>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT operation_id, result_change_group_id
            FROM code_rollback_operations
            WHERE project_id = ?1
              AND status = 'pending'
            ORDER BY created_at ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!("Xero could not prepare rollback operation validation query: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id], |row| Ok((row.get(0)?, row.get(1)?)))
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_query_failed",
                format!("Xero could not query rollback operations: {error}"),
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        CommandError::system_fault(
            "code_rollback_operation_query_failed",
            format!("Xero could not decode rollback operation rows: {error}"),
        )
    })
}

fn recover_pending_rollback_operations(
    repo_root: &Path,
    connection: &Connection,
    project_id: &str,
) -> CommandResult<Vec<CodeRollbackStorageDiagnostic>> {
    let mut diagnostics = Vec::new();
    for (operation_id, result_change_group_id) in
        collect_pending_rollback_operations(connection, project_id)?
    {
        let commits =
            collect_code_history_operation_commits(connection, project_id, &operation_id)?;
        if commits.len() == 1 {
            let commit = &commits[0];
            if change_group_status(connection, project_id, &commit.change_group_id)?.as_deref()
                == Some("completed")
            {
                let pre_snapshot_id = load_change_group_before_snapshot_id(
                    repo_root,
                    project_id,
                    &commit.change_group_id,
                )?;
                let affected_files = load_completed_code_change_files(
                    connection,
                    project_id,
                    &commit.change_group_id,
                )?;
                complete_rollback_operation(
                    repo_root,
                    project_id,
                    &operation_id,
                    &pre_snapshot_id,
                    &commit.change_group_id,
                    &affected_files,
                )?;
                diagnostics.push(CodeRollbackStorageDiagnostic {
                    code: "code_rollback_operation_recovered_completed".into(),
                    snapshot_id: None,
                    message: format!(
                        "Rollback operation `{operation_id}` completed before interruption and was recovered at startup."
                    ),
                });
                continue;
            }
        }

        let failure = CommandError::retryable(
            "code_rollback_operation_incomplete",
            format!(
                "Rollback operation `{operation_id}` was still pending at startup and needs inspection."
            ),
        );
        mark_rollback_operation_failed(connection, project_id, &operation_id, &failure)?;
        if let Some(result_change_group_id) = result_change_group_id {
            let _ =
                mark_change_group_failed(repo_root, project_id, &result_change_group_id, &failure);
        }
        diagnostics.push(CodeRollbackStorageDiagnostic {
            code: "code_rollback_operation_incomplete".into(),
            snapshot_id: None,
            message: format!(
                "Rollback operation `{operation_id}` was still pending at startup and needs inspection."
            ),
        });
    }
    Ok(diagnostics)
}

fn recover_interrupted_code_history_operations(
    repo_root: &Path,
    connection: &Connection,
    project_id: &str,
) -> CommandResult<Vec<CodeRollbackStorageDiagnostic>> {
    let mut diagnostics = Vec::new();
    for operation in collect_active_code_history_operations(connection, project_id)? {
        diagnostics.push(recover_interrupted_code_history_operation(
            repo_root, connection, project_id, &operation,
        )?);
    }
    Ok(diagnostics)
}

fn recover_interrupted_code_history_operation(
    repo_root: &Path,
    connection: &Connection,
    project_id: &str,
    operation: &CodeHistoryOperationRecoveryRow,
) -> CommandResult<CodeRollbackStorageDiagnostic> {
    let commits =
        collect_code_history_operation_commits(connection, project_id, &operation.operation_id)?;
    if commits.len() > 1 {
        let message = format!(
            "Code history operation `{}` has {} result commits and needs manual repair before it can be trusted.",
            operation.operation_id,
            commits.len()
        );
        mark_code_history_operation_repair_needed(
            connection,
            repo_root,
            project_id,
            &operation.operation_id,
            "code_history_operation_duplicate_commits",
            message.clone(),
        )?;
        return Ok(CodeRollbackStorageDiagnostic {
            code: "code_history_operation_duplicate_commits".into(),
            snapshot_id: None,
            message,
        });
    }

    if let Some(commit) = commits.first() {
        if change_group_status(connection, project_id, &commit.change_group_id)?.as_deref()
            == Some("completed")
        {
            let affected_paths = collect_affected_paths_for_change_group(
                connection,
                project_id,
                &commit.change_group_id,
            )?;
            mark_code_history_operation_completed(
                repo_root,
                project_id,
                &operation.operation_id,
                Some(&commit.change_group_id),
                Some(&commit.commit_id),
                &affected_paths,
            )?;
            return Ok(CodeRollbackStorageDiagnostic {
                code: "code_history_operation_recovered_completed".into(),
                snapshot_id: None,
                message: format!(
                    "Code history operation `{}` completed before interruption and was recovered at startup.",
                    operation.operation_id
                ),
            });
        }

        let message = format!(
            "Code history operation `{}` wrote commit `{}` but its result change group is not complete.",
            operation.operation_id, commit.commit_id
        );
        mark_code_history_operation_repair_needed(
            connection,
            repo_root,
            project_id,
            &operation.operation_id,
            "code_history_operation_commit_incomplete",
            message.clone(),
        )?;
        return Ok(CodeRollbackStorageDiagnostic {
            code: "code_history_operation_commit_incomplete".into(),
            snapshot_id: None,
            message,
        });
    }

    if operation.status == CodeHistoryOperationStatus::Applying {
        if let Some(result_change_group_id) = operation.result_change_group_id.as_deref() {
            if change_group_status(connection, project_id, result_change_group_id)?.as_deref()
                == Some("open")
            {
                let failure = CommandError::retryable(
                    "code_history_operation_interrupted",
                    format!(
                        "Code history operation `{}` was interrupted while applying.",
                        operation.operation_id
                    ),
                );
                let _ = mark_change_group_failed(
                    repo_root,
                    project_id,
                    result_change_group_id,
                    &failure,
                );
            }
        }
        let message = format!(
            "Code history operation `{}` was interrupted while applying file changes and needs workspace inspection.",
            operation.operation_id
        );
        mark_code_history_operation_repair_needed(
            connection,
            repo_root,
            project_id,
            &operation.operation_id,
            "code_history_operation_apply_interrupted",
            message.clone(),
        )?;
        return Ok(CodeRollbackStorageDiagnostic {
            code: "code_history_operation_apply_interrupted".into(),
            snapshot_id: None,
            message,
        });
    }

    if operation.result_commit_id.is_some() {
        let message = format!(
            "Code history operation `{}` references a missing result commit and needs repair.",
            operation.operation_id
        );
        mark_code_history_operation_repair_needed(
            connection,
            repo_root,
            project_id,
            &operation.operation_id,
            "code_history_operation_commit_missing",
            message.clone(),
        )?;
        return Ok(CodeRollbackStorageDiagnostic {
            code: "code_history_operation_commit_missing".into(),
            snapshot_id: None,
            message,
        });
    }

    let failure = CommandError::retryable(
        "code_history_operation_interrupted",
        format!(
            "Code history operation `{}` was interrupted before file changes were applied.",
            operation.operation_id
        ),
    );
    mark_code_history_operation_failed(
        connection,
        repo_root,
        project_id,
        &operation.operation_id,
        &failure,
    )?;
    Ok(CodeRollbackStorageDiagnostic {
        code: "code_history_operation_interrupted".into(),
        snapshot_id: None,
        message: failure.message,
    })
}

fn collect_active_code_history_operations(
    connection: &Connection,
    project_id: &str,
) -> CommandResult<Vec<CodeHistoryOperationRecoveryRow>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT operation_id, status, result_change_group_id, result_commit_id
            FROM code_history_operations
            WHERE project_id = ?1
              AND status IN ('pending', 'planning', 'applying')
            ORDER BY created_at ASC, operation_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!("Xero could not prepare code history operation recovery query: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id], |row| {
            let status = row.get::<_, String>(1)?;
            Ok((
                row.get::<_, String>(0)?,
                status,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_query_failed",
                format!("Xero could not query interrupted code history operations: {error}"),
            )
        })?;
    rows.map(|row| {
        let (operation_id, status, result_change_group_id, result_commit_id) =
            row.map_err(|error| {
                CommandError::system_fault(
                    "code_history_operation_query_failed",
                    format!("Xero could not decode code history operation row: {error}"),
                )
            })?;
        Ok(CodeHistoryOperationRecoveryRow {
            operation_id,
            status: CodeHistoryOperationStatus::from_sql(&status)?,
            result_change_group_id,
            result_commit_id,
        })
    })
    .collect()
}

fn collect_code_history_operation_commits(
    connection: &Connection,
    project_id: &str,
    operation_id: &str,
) -> CommandResult<Vec<CodeHistoryOperationCommitRecoveryRow>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT commit_id, change_group_id
            FROM code_commits
            WHERE project_id = ?1
              AND history_operation_id = ?2
            ORDER BY workspace_epoch ASC, completed_at ASC, commit_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_commit_query_failed",
                format!("Xero could not prepare history operation commit recovery query: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id, operation_id], |row| {
            Ok(CodeHistoryOperationCommitRecoveryRow {
                commit_id: row.get(0)?,
                change_group_id: row.get(1)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_history_operation_commit_query_failed",
                format!("Xero could not query history operation commits: {error}"),
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        CommandError::system_fault(
            "code_history_operation_commit_query_failed",
            format!("Xero could not decode history operation commit row: {error}"),
        )
    })
}

fn change_group_status(
    connection: &Connection,
    project_id: &str,
    change_group_id: &str,
) -> CommandResult<Option<String>> {
    connection
        .query_row(
            r#"
            SELECT status
            FROM code_change_groups
            WHERE project_id = ?1
              AND change_group_id = ?2
            "#,
            params![project_id, change_group_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(|error| {
            CommandError::system_fault(
                "code_change_group_query_failed",
                format!("Xero could not query code change group `{change_group_id}`: {error}"),
            )
        })
}

fn collect_affected_paths_for_change_group(
    connection: &Connection,
    project_id: &str,
    change_group_id: &str,
) -> CommandResult<Vec<String>> {
    let files = load_completed_code_change_files(connection, project_id, change_group_id)?;
    Ok(files
        .iter()
        .flat_map(|file| [file.path_before.as_deref(), file.path_after.as_deref()])
        .flatten()
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect())
}

fn load_completed_code_change_files(
    connection: &Connection,
    project_id: &str,
    change_group_id: &str,
) -> CommandResult<Vec<CompletedCodeChangeFile>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT path_before,
                   path_after,
                   operation,
                   before_hash,
                   after_hash,
                   explicitly_edited
            FROM code_file_versions
            WHERE project_id = ?1
              AND change_group_id = ?2
            ORDER BY id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_file_version_query_failed",
                format!("Xero could not prepare code file version recovery query: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id, change_group_id], |row| {
            let operation = row.get::<_, String>(2)?;
            let explicitly_edited = row.get::<_, i64>(5)?;
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                operation,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<String>>(4)?,
                explicitly_edited != 0,
            ))
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_file_version_query_failed",
                format!("Xero could not query code file versions for recovery: {error}"),
            )
        })?;
    rows.map(|row| {
        let (path_before, path_after, operation, before_hash, after_hash, explicitly_edited) =
            row.map_err(|error| {
                CommandError::system_fault(
                    "code_file_version_query_failed",
                    format!("Xero could not decode code file version recovery row: {error}"),
                )
            })?;
        Ok(CompletedCodeChangeFile {
            path_before,
            path_after,
            operation: parse_code_file_operation(&operation).ok_or_else(|| {
                CommandError::system_fault(
                    "code_file_version_operation_invalid",
                    format!(
                        "Code change group `{change_group_id}` recorded unsupported file operation `{operation}`."
                    ),
                )
            })?,
            before_hash,
            after_hash,
            explicitly_edited,
        })
    })
    .collect()
}

fn write_code_rollback_maintenance_report(
    repo_root: &Path,
    report: &CodeRollbackMaintenanceReport,
) -> CommandResult<()> {
    let diagnostics_dir = code_rollback_storage_dir_for_repo(repo_root).join(DIAGNOSTICS_DIR);
    fs::create_dir_all(&diagnostics_dir).map_err(|error| {
        CommandError::retryable(
            "code_rollback_diagnostics_dir_failed",
            format!(
                "Xero could not prepare code rollback diagnostics directory {}: {error}",
                diagnostics_dir.display()
            ),
        )
    })?;
    let report_json = serde_json::to_vec_pretty(report).map_err(|error| {
        CommandError::system_fault(
            "code_rollback_diagnostics_encode_failed",
            format!("Xero could not encode code rollback diagnostics: {error}"),
        )
    })?;
    let report_path = diagnostics_dir.join(MAINTENANCE_REPORT_FILE);
    let temp_path = diagnostics_dir.join(format!(".{}.tmp", generate_id("code-rollback-report")));
    fs::write(&temp_path, report_json).map_err(|error| {
        CommandError::retryable(
            "code_rollback_diagnostics_write_failed",
            format!(
                "Xero could not write code rollback diagnostics {}: {error}",
                temp_path.display()
            ),
        )
    })?;
    fs::rename(&temp_path, &report_path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        CommandError::retryable(
            "code_rollback_diagnostics_rename_failed",
            format!(
                "Xero could not finalize code rollback diagnostics {}: {error}",
                report_path.display()
            ),
        )
    })?;
    Ok(())
}

fn collect_reachable_code_blob_ids(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
) -> CommandResult<BTreeSet<String>> {
    let mut reachable = BTreeSet::new();
    for (snapshot_id, manifest_json) in collect_completed_snapshot_rows(connection, project_id)? {
        let manifest = serde_json::from_str::<CodeSnapshotManifest>(&manifest_json).map_err(
            |error| {
                CommandError::system_fault(
                    "code_snapshot_manifest_decode_failed",
                    format!(
                        "Xero could not decode code snapshot `{snapshot_id}` from {} while pruning blobs: {error}",
                        database_path.display()
                    ),
                )
            },
        )?;
        for entry in manifest.entries {
            if let Some(blob_id) = entry.blob_id {
                reachable.insert(blob_id);
            }
        }
    }

    let mut statement = connection
        .prepare(
            r#"
            SELECT blob_id
            FROM (
                SELECT file_version.before_blob_id AS blob_id
                FROM code_file_versions file_version
                JOIN code_change_groups change_group
                  ON change_group.project_id = file_version.project_id
                 AND change_group.change_group_id = file_version.change_group_id
                WHERE file_version.project_id = ?1
                  AND change_group.status <> 'failed'
                  AND file_version.before_blob_id IS NOT NULL
                UNION
                SELECT file_version.after_blob_id AS blob_id
                FROM code_file_versions file_version
                JOIN code_change_groups change_group
                  ON change_group.project_id = file_version.project_id
                 AND change_group.change_group_id = file_version.change_group_id
                WHERE file_version.project_id = ?1
                  AND change_group.status <> 'failed'
                  AND file_version.after_blob_id IS NOT NULL
            )
            ORDER BY blob_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!("Xero could not prepare code blob reachability query: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!("Xero could not query code blob reachability: {error}"),
            )
        })?;
    for row in rows {
        reachable.insert(row.map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!("Xero could not decode code blob reachability row: {error}"),
            )
        })?);
    }
    reachable.extend(collect_reachable_patch_history_blob_ids(
        connection,
        database_path,
        project_id,
    )?);
    reachable.extend(collect_code_history_conflict_blob_ids(
        connection,
        database_path,
        project_id,
    )?);
    Ok(reachable)
}

fn collect_reachable_patch_history_blob_ids(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
) -> CommandResult<BTreeSet<String>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT blob_id
            FROM (
                SELECT patch_file.base_blob_id AS blob_id
                FROM code_patch_files patch_file
                WHERE patch_file.project_id = ?1
                  AND patch_file.base_blob_id IS NOT NULL
                UNION
                SELECT patch_file.result_blob_id AS blob_id
                FROM code_patch_files patch_file
                WHERE patch_file.project_id = ?1
                  AND patch_file.result_blob_id IS NOT NULL
            )
            ORDER BY blob_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!(
                    "Xero could not prepare code patch history blob reachability query in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let rows = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!("Xero could not query code patch history blob reachability: {error}"),
            )
        })?;
    let mut reachable = BTreeSet::new();
    for row in rows {
        reachable.insert(row.map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!("Xero could not decode code patch history blob reachability row: {error}"),
            )
        })?);
    }
    Ok(reachable)
}

fn collect_code_history_conflict_blob_ids(
    connection: &Connection,
    database_path: &Path,
    project_id: &str,
) -> CommandResult<BTreeSet<String>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT operation_id, conflicts_json
            FROM code_history_operations
            WHERE project_id = ?1
              AND conflicts_json <> '[]'
            ORDER BY updated_at ASC, operation_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!(
                    "Xero could not prepare code history conflict blob reachability query in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!("Xero could not query code history conflict blob reachability: {error}"),
            )
        })?;

    let mut reachable = BTreeSet::new();
    for row in rows {
        let (operation_id, conflicts_json) = row.map_err(|error| {
            CommandError::system_fault(
                "code_blob_reachability_query_failed",
                format!("Xero could not decode code history conflict reachability row: {error}"),
            )
        })?;
        for conflict in
            decode_code_history_conflicts(&operation_id, &conflicts_json, database_path)?
        {
            for blob_id in [
                conflict.base_hash.as_deref(),
                conflict.selected_hash.as_deref(),
                conflict.current_hash.as_deref(),
            ]
            .into_iter()
            .flatten()
            {
                if is_code_blob_id(blob_id) {
                    reachable.insert(blob_id.to_string());
                }
            }
        }
    }
    Ok(reachable)
}

fn is_code_blob_id(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn collect_code_blob_rows(
    connection: &Connection,
    project_id: &str,
) -> CommandResult<Vec<CodeBlobRow>> {
    let mut statement = connection
        .prepare(
            r#"
            SELECT blob_id, size_bytes, storage_path, created_at
            FROM code_blobs
            WHERE project_id = ?1
            ORDER BY created_at ASC, blob_id ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_query_failed",
                format!("Xero could not prepare code blob retention query: {error}"),
            )
        })?;
    let rows = statement
        .query_map(params![project_id], |row| {
            let size_bytes = row.get::<_, i64>(1)?;
            Ok(CodeBlobRow {
                blob_id: row.get(0)?,
                size_bytes: size_bytes.max(0) as u64,
                storage_path: row.get(2)?,
                created_at: row.get(3)?,
            })
        })
        .map_err(|error| {
            CommandError::system_fault(
                "code_blob_query_failed",
                format!("Xero could not query code blob rows: {error}"),
            )
        })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(|error| {
        CommandError::system_fault(
            "code_blob_query_failed",
            format!("Xero could not decode code blob rows: {error}"),
        )
    })
}

enum PrunedBlobOutcome {
    Pruned,
    MetadataPrunedMissingFile,
}

fn prune_unreferenced_blob(
    connection: &Connection,
    repo_root: &Path,
    project_id: &str,
    blob: &CodeBlobRow,
) -> CommandResult<PrunedBlobOutcome> {
    let Some(relative_path) = safe_relative_path(&blob.storage_path) else {
        return Err(CommandError::system_fault(
            "code_blob_path_invalid",
            format!(
                "Code blob `{}` has invalid storage path `{}`.",
                blob.blob_id, blob.storage_path
            ),
        ));
    };
    let absolute_path = project_app_data_dir_for_repo(repo_root).join(relative_path);
    let tx = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "code_blob_prune_transaction_failed",
            format!("Xero could not start code blob prune transaction: {error}"),
        )
    })?;
    tx.execute(
        r#"
        DELETE FROM code_blobs
        WHERE project_id = ?1
          AND blob_id = ?2
        "#,
        params![project_id, blob.blob_id],
    )
    .map_err(|error| {
        CommandError::system_fault(
            "code_blob_prune_metadata_failed",
            format!(
                "Xero could not prune unreferenced code blob `{}` metadata: {error}",
                blob.blob_id
            ),
        )
    })?;

    let outcome = match fs::remove_file(&absolute_path) {
        Ok(()) => PrunedBlobOutcome::Pruned,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            PrunedBlobOutcome::MetadataPrunedMissingFile
        }
        Err(error) => {
            let _ = tx.rollback();
            return Err(CommandError::retryable(
                "code_blob_prune_file_failed",
                format!(
                    "Xero could not remove unreferenced code blob {}: {error}",
                    absolute_path.display()
                ),
            ));
        }
    };
    tx.commit().map_err(|error| {
        CommandError::system_fault(
            "code_blob_prune_commit_failed",
            format!(
                "Xero could not commit code blob `{}` prune metadata: {error}",
                blob.blob_id
            ),
        )
    })?;
    Ok(outcome)
}

fn blob_is_old_enough(created_at: &str, min_age_seconds: i64) -> CommandResult<bool> {
    if min_age_seconds <= 0 {
        return Ok(true);
    }
    let created_at = OffsetDateTime::parse(created_at, &Rfc3339).map_err(|error| {
        CommandError::system_fault(
            "code_blob_timestamp_invalid",
            format!("Code blob timestamp `{created_at}` is invalid: {error}"),
        )
    })?;
    Ok((OffsetDateTime::now_utc() - created_at).whole_seconds() >= min_age_seconds)
}

fn snapshot_manifest_root_moved(repo_root: &Path, manifest: &CodeSnapshotManifest) -> bool {
    let recorded = Path::new(&manifest.root_path);
    if recorded == repo_root {
        return false;
    }
    let current = repo_root
        .canonicalize()
        .unwrap_or_else(|_| repo_root.to_path_buf());
    if recorded == current {
        return false;
    }
    recorded
        .canonicalize()
        .map(|recorded| recorded != current)
        .unwrap_or(true)
}

fn should_include_walk_entry(entry: &DirEntry) -> bool {
    if entry.depth() == 0 {
        return true;
    }
    let Some(file_type) = entry.file_type() else {
        return true;
    };
    if !file_type.is_dir() {
        return true;
    }
    let Some(name) = entry.path().file_name().and_then(|name| name.to_str()) else {
        return true;
    };
    !DEFAULT_SKIPPED_DIRS
        .iter()
        .any(|candidate| *candidate == name)
}

fn normalize_targets(
    targets: Vec<CodeRollbackCaptureTarget>,
) -> CommandResult<Vec<CodeRollbackCaptureTarget>> {
    let mut normalized = Vec::with_capacity(targets.len());
    for target in targets {
        let path_before = normalize_optional_path(target.path_before.as_deref(), "pathBefore")?;
        let path_after = normalize_optional_path(target.path_after.as_deref(), "pathAfter")?;
        if path_before.is_none() && path_after.is_none() {
            return Err(CommandError::invalid_request("targetPath"));
        }
        normalized.push(CodeRollbackCaptureTarget {
            path_before,
            path_after,
            operation: target.operation,
            explicitly_edited: target.explicitly_edited,
        });
    }
    Ok(normalized)
}

fn explicit_paths_for_targets(targets: &[CodeRollbackCaptureTarget]) -> BTreeSet<String> {
    targets
        .iter()
        .flat_map(|target| {
            [target.path_before.as_deref(), target.path_after.as_deref()]
                .into_iter()
                .flatten()
        })
        .map(str::to_owned)
        .collect()
}

fn target_overlaps_explicit_paths(
    before_entry: Option<&CodeSnapshotFileEntry>,
    after_entry: Option<&CodeSnapshotFileEntry>,
    explicit_paths: &BTreeSet<String>,
) -> bool {
    [before_entry, after_entry]
        .into_iter()
        .flatten()
        .any(|entry| explicit_paths.contains(&entry.path))
}

fn broad_capture_explicit_paths(
    repo_root: &Path,
    project_id: &str,
) -> CommandResult<BTreeSet<String>> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    read_project_row(&connection, &database_path, repo_root, project_id)?;
    let mut statement = connection
        .prepare(
            r#"
            SELECT path
            FROM (
                SELECT path_before AS path
                FROM code_file_versions
                WHERE project_id = ?1
                  AND explicitly_edited = 1
                  AND generated = 1
                  AND path_before IS NOT NULL
                UNION
                SELECT path_after AS path
                FROM code_file_versions
                WHERE project_id = ?1
                  AND explicitly_edited = 1
                  AND generated = 1
                  AND path_after IS NOT NULL
            )
            ORDER BY path ASC
            "#,
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_broad_capture_explicit_paths_prepare_failed",
                format!(
                    "Xero could not prepare broad capture explicit path query in {}: {error}",
                    database_path.display()
                ),
            )
        })?;

    let rows = statement
        .query_map(params![project_id], |row| row.get::<_, String>(0))
        .map_err(|error| {
            CommandError::system_fault(
                "code_broad_capture_explicit_paths_query_failed",
                format!(
                    "Xero could not query broad capture explicit paths in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    let mut paths = BTreeSet::new();
    for row in rows {
        let path = row.map_err(|error| {
            CommandError::system_fault(
                "code_broad_capture_explicit_paths_decode_failed",
                format!(
                    "Xero could not decode broad capture explicit path in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
        if safe_relative_path(&path).is_some() {
            paths.insert(path);
        }
    }

    Ok(paths)
}

fn normalize_optional_path(
    value: Option<&str>,
    field: &'static str,
) -> CommandResult<Option<String>> {
    match value {
        Some(value) => {
            let Some(path) = safe_relative_path(value) else {
                return Err(CommandError::invalid_request(field));
            };
            Ok(Some(path_to_forward_slash(&path)))
        }
        None => Ok(None),
    }
}

fn infer_file_operation(
    target: &CodeRollbackCaptureTarget,
    before_entry: Option<&CodeSnapshotFileEntry>,
    after_entry: Option<&CodeSnapshotFileEntry>,
) -> CodeFileOperation {
    match (before_entry, after_entry) {
        (None, Some(_)) => CodeFileOperation::Create,
        (Some(_), None) => CodeFileOperation::Delete,
        (Some(before), Some(after))
            if target.path_before != target.path_after
                && target.path_before.is_some()
                && target.path_after.is_some() =>
        {
            CodeFileOperation::Rename
        }
        (Some(before), Some(after))
            if before.kind == CodeSnapshotFileKind::Symlink
                || after.kind == CodeSnapshotFileKind::Symlink =>
        {
            CodeFileOperation::SymlinkChange
        }
        (Some(before), Some(after)) if before.mode != after.mode => CodeFileOperation::ModeChange,
        _ => CodeFileOperation::Modify,
    }
}

fn validate_change_group_input(input: &CodeChangeGroupInput) -> CommandResult<()> {
    validate_non_empty(&input.project_id, "projectId")?;
    validate_non_empty(&input.agent_session_id, "agentSessionId")?;
    validate_non_empty(&input.run_id, "runId")?;
    validate_non_empty(&input.summary_label, "summaryLabel")?;
    if let Some(change_group_id) = input.change_group_id.as_deref() {
        validate_non_empty(change_group_id, "changeGroupId")?;
    }
    if let Some(parent_change_group_id) = input.parent_change_group_id.as_deref() {
        validate_non_empty(parent_change_group_id, "parentChangeGroupId")?;
    }
    if let Some(tool_call_id) = input.tool_call_id.as_deref() {
        validate_non_empty(tool_call_id, "toolCallId")?;
    }
    if input.runtime_event_id.is_some_and(|id| id <= 0) {
        return Err(CommandError::invalid_request("runtimeEventId"));
    }
    if input
        .conversation_sequence
        .is_some_and(|sequence| sequence < 0)
    {
        return Err(CommandError::invalid_request("conversationSequence"));
    }
    Ok(())
}

fn validate_non_empty(value: &str, field: &'static str) -> CommandResult<()> {
    if value.trim().is_empty() {
        return Err(CommandError::invalid_request(field));
    }
    Ok(())
}

fn blob_relative_path(blob_id: &str) -> PathBuf {
    PathBuf::from(CODE_ROLLBACK_DIR)
        .join(BLOB_DIR)
        .join(&blob_id[0..2])
        .join(blob_id)
}

fn absolute_repo_path(repo_root: &Path, relative_path: &str) -> CommandResult<PathBuf> {
    let Some(relative_path) = safe_relative_path(relative_path) else {
        return Err(CommandError::invalid_request("path"));
    };
    Ok(repo_root.join(relative_path))
}

fn repo_relative_path(repo_root: &Path, path: &Path) -> CommandResult<String> {
    let relative = path.strip_prefix(repo_root).map_err(|error| {
        CommandError::system_fault(
            "code_snapshot_path_failed",
            format!(
                "Xero could not make {} relative to {}: {error}",
                path.display(),
                repo_root.display()
            ),
        )
    })?;
    Ok(path_to_forward_slash(relative))
}

fn safe_relative_path(value: &str) -> Option<PathBuf> {
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

fn path_to_forward_slash(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(segment) => segment.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn is_generated_or_ignored_path(path: &str) -> bool {
    path.split('/').any(|segment| {
        DEFAULT_SKIPPED_DIRS
            .iter()
            .any(|candidate| *candidate == segment)
    })
}

#[cfg(test)]
fn path_depth(path: &str) -> usize {
    path.split('/').count()
}

fn file_mode(metadata: &fs::Metadata) -> Option<u32> {
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

fn set_mode_if_supported(path: &Path, mode: Option<u32>) -> CommandResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Some(mode) = mode {
            fs::set_permissions(path, fs::Permissions::from_mode(mode)).map_err(|error| {
                CommandError::retryable(
                    "code_rollback_restore_mode_failed",
                    format!("Xero could not restore mode on {}: {error}", path.display()),
                )
            })?;
        }
    }

    #[cfg(not(unix))]
    {
        let _ = path;
        let _ = mode;
    }
    Ok(())
}

fn modified_at_millis(metadata: &fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
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

fn generate_id(prefix: &str) -> String {
    let mut bytes = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut bytes);
    let random = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    format!("{prefix}-{millis}-{random}")
}

fn saturating_usize_to_i64(value: usize) -> i64 {
    value.min(i64::MAX as usize) as i64
}

fn saturating_u64_to_i64(value: u64) -> i64 {
    value.min(i64::MAX as u64) as i64
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use serde_json::json;
    use tempfile::{tempdir, TempDir};

    use super::*;
    use crate::{
        commands::RuntimeAgentIdDto,
        db::{self, project_store::DEFAULT_AGENT_SESSION_ID, ProjectOrigin},
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
        app_data_dir: PathBuf,
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
            db::project_store::insert_agent_run(
                &repo_root,
                &db::project_store::NewAgentRunRecord {
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
                    now: now_timestamp(),
                },
            )
            .expect("insert run");

            Self {
                _tempdir: tempdir,
                repo_root,
                project_id,
                agent_session_id: DEFAULT_AGENT_SESSION_ID.into(),
                run_id,
                app_data_dir,
            }
        }

        fn input(&self, label: &str) -> CodeChangeGroupInput {
            self.input_for_session_run_kind(
                label,
                &self.agent_session_id,
                &self.run_id,
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

    fn capture_modify(
        project: &TestProject,
        label: &str,
        path: &str,
        after: &str,
    ) -> CompletedCodeChangeGroup {
        let handle = begin_exact_path_capture(
            &project.repo_root,
            project.input(label),
            vec![CodeRollbackCaptureTarget::modify(path)],
        )
        .expect("begin modify capture");
        fs::write(project.repo_root.join(path), after).expect("write changed file");
        complete_exact_path_capture(&project.repo_root, handle).expect("complete modify capture")
    }

    fn create_test_agent_session(project: &TestProject, title: &str) -> String {
        db::project_store::create_agent_session(
            &project.repo_root,
            &db::project_store::AgentSessionCreateRecord {
                project_id: project.project_id.clone(),
                title: title.into(),
                summary: String::new(),
                selected: false,
            },
        )
        .expect("create agent session")
        .agent_session_id
    }

    fn insert_test_agent_run(
        project: &TestProject,
        agent_session_id: &str,
        run_id: &str,
        now: &str,
    ) {
        db::project_store::insert_agent_run(
            &project.repo_root,
            &db::project_store::NewAgentRunRecord {
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

    fn activate_test_run(project: &TestProject, run_id: &str, summary: &str) {
        let now = now_timestamp();
        db::project_store::upsert_agent_coordination_presence(
            &project.repo_root,
            &db::project_store::UpsertAgentCoordinationPresenceRecord {
                project_id: project.project_id.clone(),
                run_id: run_id.into(),
                pane_id: None,
                status: "running".into(),
                current_phase: "test".into(),
                activity_summary: summary.into(),
                last_event_id: None,
                last_event_kind: None,
                updated_at: now,
                lease_seconds: Some(3_600),
            },
        )
        .expect("activate coordination presence");
    }

    fn claim_test_reservation(project: &TestProject, run_id: &str, path: &str) {
        let now = now_timestamp();
        let result = db::project_store::claim_agent_file_reservations(
            &project.repo_root,
            &db::project_store::ClaimAgentFileReservationRequest {
                project_id: project.project_id.clone(),
                owner_run_id: run_id.into(),
                paths: vec![path.into()],
                operation: db::project_store::AgentCoordinationReservationOperation::Editing,
                note: Some(format!("Editing {path}")),
                override_reason: None,
                claimed_at: now,
                lease_seconds: Some(3_600),
            },
        )
        .expect("claim reservation");
        assert_eq!(result.claimed.len(), 1);
        assert!(result.conflicts.is_empty());
    }

    fn append_test_recent_path_activity(project: &TestProject, run_id: &str, path: &str) {
        db::project_store::append_agent_coordination_event(
            &project.repo_root,
            &db::project_store::NewAgentCoordinationEventRecord {
                project_id: project.project_id.clone(),
                run_id: run_id.into(),
                event_kind: "file_changed".into(),
                summary: format!("Changed `{path}`."),
                payload: json!({
                    "path": path,
                    "operation": "modified"
                }),
                created_at: now_timestamp(),
                lease_seconds: Some(3_600),
            },
        )
        .expect("append recent path activity");
    }

    fn capture_modify_for_session_run(
        project: &TestProject,
        agent_session_id: &str,
        run_id: &str,
        label: &str,
        path: &str,
        after: &str,
    ) -> CompletedCodeChangeGroup {
        let handle = begin_exact_path_capture(
            &project.repo_root,
            project.input_for_session_run_kind(
                label,
                agent_session_id,
                run_id,
                CodeChangeKind::FileTool,
            ),
            vec![CodeRollbackCaptureTarget::modify(path)],
        )
        .expect("begin modify capture");
        fs::write(project.repo_root.join(path), after).expect("write changed file");
        complete_exact_path_capture(&project.repo_root, handle).expect("complete modify capture")
    }

    fn replace_text_patch_hunks(
        project: &TestProject,
        change_group_id: &str,
        path: &str,
        hunks: Vec<CodePatchHunkInput>,
    ) -> String {
        let metadata = db::project_store::read_code_change_group_history_metadata(
            &project.repo_root,
            &project.project_id,
            change_group_id,
        )
        .expect("read history metadata")
        .expect("history metadata");
        let commit = db::project_store::read_code_patchset_commit(
            &project.repo_root,
            &project.project_id,
            metadata.commit_id.as_deref().expect("commit id"),
        )
        .expect("read patchset commit")
        .expect("patchset commit");
        let patch_file = commit
            .files
            .iter()
            .find(|file| file.path_after.as_deref() == Some(path))
            .expect("text patch file");
        let patch_file_id = patch_file.patch_file_id.clone();
        let patchset_id = commit.patchset.patchset_id.clone();
        let connection =
            Connection::open(db::database_path_for_repo(&project.repo_root)).expect("open db");
        connection
            .execute(
                "DELETE FROM code_patch_hunks WHERE project_id = ?1 AND patch_file_id = ?2",
                params![project.project_id, patch_file_id.as_str()],
            )
            .expect("delete existing hunk rows");
        for hunk in &hunks {
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
                        project.project_id,
                        patch_file_id.as_str(),
                        hunk.hunk_id,
                        i64::from(hunk.hunk_index),
                        i64::from(hunk.base_start_line),
                        i64::from(hunk.base_line_count),
                        i64::from(hunk.result_start_line),
                        i64::from(hunk.result_line_count),
                        serde_json::to_string(&hunk.removed_lines).expect("removed lines json"),
                        serde_json::to_string(&hunk.added_lines).expect("added lines json"),
                        serde_json::to_string(&hunk.context_before).expect("context before json"),
                        serde_json::to_string(&hunk.context_after).expect("context after json"),
                        "2026-05-06T12:00:00Z",
                    ],
                )
                .expect("insert replacement hunk row");
        }
        connection
            .execute(
                "UPDATE code_patch_files SET text_hunk_count = ?3 WHERE project_id = ?1 AND patch_file_id = ?2",
                params![project.project_id, patch_file_id.as_str(), hunks.len() as i64],
            )
            .expect("update patch file hunk count");
        connection
            .execute(
                "UPDATE code_patchsets SET text_hunk_count = ?3 WHERE project_id = ?1 AND patchset_id = ?2",
                params![project.project_id, patchset_id, hunks.len() as i64],
            )
            .expect("update patchset hunk count");
        patch_file_id
    }

    fn insert_orphan_blob(project: &TestProject, blob_id: &str) -> PathBuf {
        let relative_path = blob_relative_path(blob_id);
        let absolute_path = project_app_data_dir_for_repo(&project.repo_root).join(&relative_path);
        fs::create_dir_all(absolute_path.parent().expect("blob parent")).expect("blob dir");
        fs::write(&absolute_path, b"orphan blob").expect("orphan blob file");
        let connection = Connection::open(db::database_path_for_repo(&project.repo_root))
            .expect("open project db");
        connection
            .execute(
                r#"
                INSERT INTO code_blobs (
                    project_id,
                    blob_id,
                    sha256,
                    size_bytes,
                    storage_path,
                    compression,
                    created_at
                )
                VALUES (?1, ?2, ?2, ?3, ?4, 'none', ?5)
                "#,
                params![
                    project.project_id,
                    blob_id,
                    11_i64,
                    path_to_forward_slash(&relative_path),
                    now_timestamp(),
                ],
            )
            .expect("insert orphan blob row");
        absolute_path
    }

    fn insert_test_blob(project: &TestProject, bytes: &[u8]) -> (String, PathBuf) {
        let blob_id = sha256_hex(bytes);
        let relative_path = blob_relative_path(&blob_id);
        let absolute_path = project_app_data_dir_for_repo(&project.repo_root).join(&relative_path);
        fs::create_dir_all(absolute_path.parent().expect("blob parent")).expect("blob dir");
        fs::write(&absolute_path, bytes).expect("test blob file");
        let connection = Connection::open(db::database_path_for_repo(&project.repo_root))
            .expect("open project db");
        connection
            .execute(
                r#"
                INSERT INTO code_blobs (
                    project_id,
                    blob_id,
                    sha256,
                    size_bytes,
                    storage_path,
                    compression,
                    created_at
                )
                VALUES (?1, ?2, ?2, ?3, ?4, 'none', ?5)
                "#,
                params![
                    project.project_id,
                    blob_id.as_str(),
                    bytes.len() as i64,
                    path_to_forward_slash(&relative_path),
                    now_timestamp(),
                ],
            )
            .expect("insert test blob row");
        (blob_id, absolute_path)
    }

    fn rewrite_snapshot_root(project: &TestProject, snapshot_id: &str, root_path: &str) {
        let connection = Connection::open(db::database_path_for_repo(&project.repo_root))
            .expect("open project db");
        let manifest_json: String = connection
            .query_row(
                "SELECT manifest_json FROM code_snapshots WHERE project_id = ?1 AND snapshot_id = ?2",
                params![project.project_id, snapshot_id],
                |row| row.get(0),
            )
            .expect("snapshot manifest");
        let mut manifest: CodeSnapshotManifest =
            serde_json::from_str(&manifest_json).expect("decode manifest");
        manifest.root_path = root_path.into();
        let updated = serde_json::to_string(&manifest).expect("encode manifest");
        connection
            .execute(
                "UPDATE code_snapshots SET manifest_json = ?3 WHERE project_id = ?1 AND snapshot_id = ?2",
                params![project.project_id, snapshot_id, updated],
            )
            .expect("rewrite snapshot root");
    }

    #[test]
    #[cfg(unix)]
    fn exact_path_capture_restores_modify_create_delete_rename_binary_and_symlink() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_exact");
        let root = &project.repo_root;
        fs::create_dir_all(root.join("src")).expect("src");
        fs::write(root.join("src/modify.txt"), "before\n").expect("modify");
        fs::write(root.join("delete.txt"), "delete me\n").expect("delete");
        fs::write(root.join("old-name.txt"), "rename me\n").expect("rename");
        fs::write(root.join("binary.dat"), [0_u8, 159, 146, 150]).expect("binary");
        std::os::unix::fs::symlink("target-v1", root.join("link")).expect("symlink");

        let handle = begin_exact_path_capture(
            root,
            project.input("exact restore"),
            vec![
                CodeRollbackCaptureTarget::modify("src/modify.txt"),
                CodeRollbackCaptureTarget::create("created.txt"),
                CodeRollbackCaptureTarget::delete("delete.txt"),
                CodeRollbackCaptureTarget::rename("old-name.txt", "new-name.txt"),
                CodeRollbackCaptureTarget::modify("binary.dat"),
                CodeRollbackCaptureTarget::symlink_change("link"),
                CodeRollbackCaptureTarget::create("created-dir"),
            ],
        )
        .expect("begin capture");

        fs::write(root.join("src/modify.txt"), "after\n").expect("modify after");
        fs::write(root.join("created.txt"), "created\n").expect("created");
        fs::remove_file(root.join("delete.txt")).expect("delete file");
        fs::rename(root.join("old-name.txt"), root.join("new-name.txt")).expect("rename");
        fs::write(root.join("binary.dat"), [9_u8, 8, 7, 6]).expect("binary after");
        fs::remove_file(root.join("link")).expect("remove link");
        std::os::unix::fs::symlink("target-v2", root.join("link")).expect("symlink v2");
        fs::create_dir(root.join("created-dir")).expect("created dir");

        let completed = complete_exact_path_capture(root, handle).expect("complete capture");
        assert_eq!(completed.file_version_count, 7);

        let head = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        assert_eq!(head.workspace_epoch, 1);
        let commit = db::project_store::read_code_patchset_commit(
            root,
            &project.project_id,
            head.head_id.as_deref().expect("head commit id"),
        )
        .expect("read exact capture commit")
        .expect("exact capture commit");
        assert_eq!(commit.commit.change_group_id, completed.change_group_id);
        assert_eq!(commit.commit.workspace_epoch, 1);
        assert_eq!(commit.patchset.file_count, 7);
        assert_eq!(commit.files.len(), 7);
        assert!(commit
            .files
            .iter()
            .any(|file| file.operation == CodePatchFileOperation::Delete
                && file.path_before.as_deref() == Some("delete.txt")));
        assert!(commit
            .files
            .iter()
            .any(|file| file.operation == CodePatchFileOperation::Rename
                && file.path_before.as_deref() == Some("old-name.txt")
                && file.path_after.as_deref() == Some("new-name.txt")));
        let text_modify = commit
            .files
            .iter()
            .find(|file| file.path_after.as_deref() == Some("src/modify.txt"))
            .expect("text modify patch file");
        assert_eq!(text_modify.merge_policy, CodePatchMergePolicy::Text);
        assert_eq!(text_modify.hunks.len(), 1);
        assert_eq!(text_modify.hunks[0].removed_lines, vec!["before\n"]);
        assert_eq!(text_modify.hunks[0].added_lines, vec!["after\n"]);
        let binary_modify = commit
            .files
            .iter()
            .find(|file| file.path_after.as_deref() == Some("binary.dat"))
            .expect("binary modify patch file");
        assert_eq!(binary_modify.merge_policy, CodePatchMergePolicy::Exact);
        assert!(binary_modify.hunks.is_empty());
        let directory_create = commit
            .files
            .iter()
            .find(|file| file.path_after.as_deref() == Some("created-dir"))
            .expect("directory create patch file");
        assert_eq!(directory_create.operation, CodePatchFileOperation::Create);
        assert_eq!(
            directory_create.after_file_kind,
            Some(CodePatchFileKind::Directory)
        );
        assert_eq!(directory_create.merge_policy, CodePatchMergePolicy::Exact);
        let old_name_epoch =
            db::project_store::read_code_path_epoch(root, &project.project_id, "old-name.txt")
                .expect("read old-name path epoch")
                .expect("old-name path epoch");
        let new_name_epoch =
            db::project_store::read_code_path_epoch(root, &project.project_id, "new-name.txt")
                .expect("read new-name path epoch")
                .expect("new-name path epoch");
        assert_eq!(old_name_epoch.workspace_epoch, 1);
        assert_eq!(new_name_epoch.workspace_epoch, 1);
        assert_eq!(old_name_epoch.commit_id.as_deref(), head.head_id.as_deref());
        assert_eq!(
            new_name_epoch.commit_id.as_deref(),
            old_name_epoch.commit_id.as_deref()
        );

        fs::write(root.join("src/modify.txt"), "later user edit\n").expect("later modify");
        fs::write(root.join("created.txt"), "later created edit\n").expect("later created");
        fs::write(root.join("new-name.txt"), "later rename edit\n").expect("later rename");
        fs::write(root.join("binary.dat"), [1_u8, 2, 3]).expect("later binary");
        fs::remove_file(root.join("link")).expect("remove link later");
        std::os::unix::fs::symlink("target-user", root.join("link")).expect("symlink user");
        assert!(root.join("created-dir").is_dir());
        fs::write(root.join("extra.txt"), "remove me\n").expect("extra");

        let outcome =
            restore_code_snapshot(root, &project.project_id, &completed.before_snapshot_id)
                .expect("restore snapshot");

        assert_eq!(
            fs::read_to_string(root.join("src/modify.txt")).expect("read modify"),
            "before\n"
        );
        assert!(!root.join("created.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("delete.txt")).expect("read delete"),
            "delete me\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("old-name.txt")).expect("read old name"),
            "rename me\n"
        );
        assert!(!root.join("new-name.txt").exists());
        assert_eq!(
            fs::read(root.join("binary.dat")).expect("read binary"),
            vec![0_u8, 159, 146, 150]
        );
        assert_eq!(
            fs::read_link(root.join("link"))
                .expect("read link")
                .to_string_lossy(),
            "target-v1"
        );
        assert!(!root.join("created-dir").exists());
        assert!(!root.join("extra.txt").exists());
        assert!(outcome.removed_paths.iter().any(|path| path == "extra.txt"));
        assert!(code_rollback_storage_dir_for_repo(root).starts_with(
            project
                .app_data_dir
                .join("projects")
                .join(&project.project_id)
        ));
        assert!(code_rollback_storage_dir_for_repo(root)
            .join(BLOB_DIR)
            .is_dir());
        assert!(!root.join(".xero").exists());
    }

    #[test]
    #[cfg(unix)]
    fn restore_reinstates_unix_file_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_permissions");
        let root = &project.repo_root;
        fs::write(root.join("script.sh"), "#!/bin/sh\n").expect("script");
        fs::set_permissions(root.join("script.sh"), fs::Permissions::from_mode(0o640))
            .expect("initial mode");

        let handle = begin_exact_path_capture(
            root,
            project.input("mode change"),
            vec![CodeRollbackCaptureTarget {
                path_before: Some("script.sh".into()),
                path_after: Some("script.sh".into()),
                operation: Some(CodeFileOperation::ModeChange),
                explicitly_edited: true,
            }],
        )
        .expect("begin mode capture");
        fs::set_permissions(root.join("script.sh"), fs::Permissions::from_mode(0o600))
            .expect("changed mode");
        let completed = complete_exact_path_capture(root, handle).expect("complete mode capture");

        fs::set_permissions(root.join("script.sh"), fs::Permissions::from_mode(0o644))
            .expect("later mode");
        restore_code_snapshot(root, &project.project_id, &completed.before_snapshot_id)
            .expect("restore mode snapshot");

        let restored_mode = fs::metadata(root.join("script.sh"))
            .expect("script metadata")
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(restored_mode, 0o640);
    }

    #[test]
    fn broad_capture_records_manifest_diff() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_broad");
        let root = &project.repo_root;
        fs::write(root.join("one.txt"), "one\n").expect("one");
        fs::write(root.join("two.txt"), "two\n").expect("two");

        let handle = begin_broad_capture(
            root,
            CodeChangeGroupInput {
                change_kind: CodeChangeKind::Command,
                restore_state: CodeChangeRestoreState::SnapshotAvailable,
                ..project.input("broad")
            },
        )
        .expect("begin broad");
        fs::write(root.join("one.txt"), "one changed\n").expect("change one");
        fs::write(root.join("three.txt"), "three\n").expect("create three");

        let completed = complete_broad_capture(root, handle).expect("complete broad");

        assert_eq!(completed.file_version_count, 2);
        let restored =
            restore_code_snapshot(root, &project.project_id, &completed.before_snapshot_id)
                .expect("restore broad");
        assert_eq!(
            fs::read_to_string(root.join("one.txt")).expect("read one"),
            "one\n"
        );
        assert!(!root.join("three.txt").exists());
        assert!(restored
            .removed_paths
            .iter()
            .any(|path| path == "three.txt"));
    }

    #[test]
    fn broad_capture_large_repository_records_single_diff_and_skips_generated_dirs() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_large_broad");
        let root = &project.repo_root;
        fs::create_dir_all(root.join("src")).expect("src");
        fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules");
        for index in 0..300 {
            fs::write(
                root.join(format!("src/file-{index:03}.txt")),
                format!("unchanged {index}\n"),
            )
            .expect("fixture file");
        }
        fs::write(root.join("node_modules/pkg/generated.txt"), "ignored\n").expect("ignored");

        let handle = begin_broad_capture(
            root,
            CodeChangeGroupInput {
                change_kind: CodeChangeKind::Command,
                restore_state: CodeChangeRestoreState::SnapshotAvailable,
                ..project.input("large broad")
            },
        )
        .expect("begin broad");
        fs::write(
            root.join("src/file-177.txt"),
            "changed with a different size\n",
        )
        .expect("change one");
        fs::write(
            root.join("node_modules/pkg/generated.txt"),
            "ignored changed\n",
        )
        .expect("ignored changed");

        let completed = complete_broad_capture(root, handle).expect("complete broad");

        assert_eq!(completed.file_version_count, 1);
        assert_eq!(
            completed.affected_files[0].path_before.as_deref(),
            Some("src/file-177.txt")
        );
        let (connection, database_path) = open_code_rollback_database(root).expect("db");
        let after_manifest = load_completed_snapshot_manifest(
            &connection,
            &database_path,
            &project.project_id,
            &completed.after_snapshot_id,
        )
        .expect("after manifest");
        assert!(!after_manifest
            .entries
            .iter()
            .any(|entry| entry.path.starts_with("node_modules/")));
    }

    #[test]
    fn broad_capture_budgets_diff_payloads_and_preserves_explicit_generated_paths() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_broad_budget_explicit");
        let root = &project.repo_root;
        fs::create_dir_all(root.join("src")).expect("src");
        fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules");
        for index in 0..8 {
            fs::write(
                root.join(format!("src/file-{index:03}.txt")),
                format!("before {index}\n"),
            )
            .expect("fixture file");
        }
        fs::write(
            root.join("node_modules/pkg/explicit.txt"),
            "explicit before\n",
        )
        .expect("explicit baseline");

        let explicit_handle = begin_exact_path_capture(
            root,
            project.input("explicit generated"),
            vec![CodeRollbackCaptureTarget::modify(
                "node_modules/pkg/explicit.txt",
            )],
        )
        .expect("begin explicit capture");
        fs::write(
            root.join("node_modules/pkg/explicit.txt"),
            "explicit tracked\n",
        )
        .expect("track explicit");
        complete_exact_path_capture(root, explicit_handle).expect("complete explicit capture");

        let scan_budget = CodeSnapshotScanBudget {
            max_walk_entries: 4,
            max_new_blob_bytes: 32,
        };
        let patch_budget = CodePatchPayloadBudget {
            max_non_explicit_diff_files: 1,
            max_text_hunk_payload_bytes: DEFAULT_TEXT_HUNK_PAYLOAD_BYTES,
        };
        let handle = begin_broad_capture_with_budget(
            root,
            CodeChangeGroupInput {
                change_kind: CodeChangeKind::Command,
                restore_state: CodeChangeRestoreState::SnapshotAvailable,
                ..project.input("budgeted broad")
            },
            scan_budget,
        )
        .expect("begin budgeted broad");
        for index in 0..8 {
            fs::write(
                root.join(format!("src/file-{index:03}.txt")),
                format!("after {index}\n"),
            )
            .expect("change fixture file");
        }
        fs::write(
            root.join("node_modules/pkg/explicit.txt"),
            "explicit final\n",
        )
        .expect("change explicit");

        let completed =
            complete_broad_capture_with_budgets(root, handle, scan_budget, patch_budget)
                .expect("complete budgeted broad");

        assert!(completed.affected_files.iter().any(|file| {
            file.path_after.as_deref() == Some("node_modules/pkg/explicit.txt")
                && file.explicitly_edited
        }));
        let non_explicit_count = completed
            .affected_files
            .iter()
            .filter(|file| !file.explicitly_edited)
            .count();
        assert!(non_explicit_count <= 1);

        let metadata = completed.history_metadata.expect("history metadata");
        let commit = db::project_store::read_code_patchset_commit(
            root,
            &project.project_id,
            metadata.commit_id.as_deref().expect("commit id"),
        )
        .expect("read budgeted commit")
        .expect("budgeted commit");
        assert!(commit
            .files
            .iter()
            .any(|file| { file.path_after.as_deref() == Some("node_modules/pkg/explicit.txt") }));
        let non_explicit_patch_files = commit
            .files
            .iter()
            .filter(|file| {
                file.path_after.as_deref() != Some("node_modules/pkg/explicit.txt")
                    && file.path_before.as_deref() != Some("node_modules/pkg/explicit.txt")
            })
            .count();
        assert!(non_explicit_patch_files <= 1);
    }

    #[test]
    fn broad_capture_downgrades_large_text_hunks_to_exact_patch_payloads() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_broad_hunk_budget");
        let root = &project.repo_root;
        fs::write(root.join("large.txt"), format!("{}\n", "a".repeat(96))).expect("large baseline");

        let patch_budget = CodePatchPayloadBudget {
            max_non_explicit_diff_files: 10,
            max_text_hunk_payload_bytes: 32,
        };
        let handle = begin_broad_capture_with_budget(
            root,
            CodeChangeGroupInput {
                change_kind: CodeChangeKind::Command,
                restore_state: CodeChangeRestoreState::SnapshotAvailable,
                ..project.input("hunk budget")
            },
            CodeSnapshotScanBudget::default(),
        )
        .expect("begin hunk budget capture");
        fs::write(root.join("large.txt"), format!("{}\n", "b".repeat(96))).expect("large change");

        let completed = complete_broad_capture_with_budgets(
            root,
            handle,
            CodeSnapshotScanBudget::default(),
            patch_budget,
        )
        .expect("complete hunk budget capture");
        let metadata = completed.history_metadata.expect("history metadata");
        let commit = db::project_store::read_code_patchset_commit(
            root,
            &project.project_id,
            metadata.commit_id.as_deref().expect("commit id"),
        )
        .expect("read hunk budget commit")
        .expect("hunk budget commit");
        assert_eq!(commit.files.len(), 1);
        assert_eq!(commit.files[0].merge_policy, CodePatchMergePolicy::Exact);
        assert!(commit.files[0].hunks.is_empty());
        assert!(commit.files[0].base_blob_id.is_some());
        assert!(commit.files[0].result_blob_id.is_some());
    }

    #[test]
    fn apply_code_rollback_uses_change_group_undo_and_preserves_unrelated_current_changes() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_alias_preserves_unrelated");
        let root = &project.repo_root;
        fs::write(root.join("tracked.txt"), "A\n").expect("baseline");
        fs::write(root.join("unrelated.txt"), "unrelated before\n").expect("unrelated");

        let group = capture_modify(&project, "edit tracked", "tracked.txt", "B\n");
        fs::write(root.join("unrelated.txt"), "user unrelated edit\n").expect("human edit");

        let rollback = apply_code_rollback(root, &project.project_id, &group.change_group_id)
            .expect("apply rollback");

        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read rolled back file"),
            "A\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("unrelated.txt")).expect("read unrelated file"),
            "user unrelated edit\n"
        );
        assert_eq!(rollback.target_snapshot_id, group.before_snapshot_id);
        assert_eq!(rollback.affected_files.len(), 1);
        assert_eq!(
            rollback.affected_files[0].path_before.as_deref(),
            Some("tracked.txt")
        );
        assert_eq!(
            rollback.affected_files[0].path_after.as_deref(),
            Some("tracked.txt")
        );
        assert_eq!(rollback.restored_paths, vec!["tracked.txt"]);
        assert!(rollback.removed_paths.is_empty());

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (operation_status, affected_count): (String, i64) = connection
            .query_row(
                r#"
                SELECT status, json_array_length(affected_files_json)
                FROM code_rollback_operations
                WHERE project_id = ?1
                  AND operation_id = ?2
                "#,
                params![project.project_id, rollback.operation_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("rollback operation row");
        assert_eq!(operation_status, "completed");
        assert_eq!(affected_count, 1);
        let operations = list_code_rollback_operations_for_session(
            root,
            &project.project_id,
            &project.agent_session_id,
            Some(&project.run_id),
        )
        .expect("list rollback operations");
        assert_eq!(operations.len(), 1);
        assert_eq!(operations[0].operation_id, rollback.operation_id);
        assert_eq!(
            operations[0].target_summary_label.as_deref(),
            Some("edit tracked")
        );
        assert!(operations[0]
            .affected_files
            .iter()
            .any(|file| file.path_before.as_deref() == Some("tracked.txt")));
        let recent = list_recent_code_rollback_operations_for_session(
            root,
            &project.project_id,
            &project.agent_session_id,
            5,
        )
        .expect("recent rollback operations");
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].operation_id, rollback.operation_id);

        let (change_kind, status, parent_change_group_id, before_snapshot_id): (
            String,
            String,
            String,
            String,
        ) = connection
            .query_row(
                r#"
                SELECT change_kind, status, parent_change_group_id, before_snapshot_id
                FROM code_change_groups
                WHERE project_id = ?1
                  AND change_group_id = ?2
                "#,
                params![project.project_id, rollback.result_change_group_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("rollback change group row");
        assert_eq!(change_kind, "rollback");
        assert_eq!(status, "completed");
        assert_eq!(parent_change_group_id, group.change_group_id);
        assert_eq!(before_snapshot_id, rollback.pre_rollback_snapshot_id);

        let metadata = db::project_store::read_code_change_group_history_metadata(
            root,
            &project.project_id,
            &rollback.result_change_group_id,
        )
        .expect("read rollback alias metadata")
        .expect("rollback alias metadata");
        let commit = db::project_store::read_code_patchset_commit(
            root,
            &project.project_id,
            metadata
                .commit_id
                .as_deref()
                .expect("rollback alias commit id"),
        )
        .expect("read rollback alias commit")
        .expect("rollback alias commit");
        assert_eq!(commit.commit.commit_kind, CodeHistoryCommitKind::Undo);
        assert_eq!(
            commit.commit.history_operation_id.as_deref(),
            Some(rollback.operation_id.as_str())
        );

        let redo = apply_code_rollback(root, &project.project_id, &rollback.result_change_group_id)
            .expect("undo the compatibility undo");
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read redone file"),
            "B\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("unrelated.txt")).expect("read unrelated after redo"),
            "user unrelated edit\n"
        );
        assert_eq!(redo.target_snapshot_id, rollback.pre_rollback_snapshot_id);
    }

    #[test]
    fn apply_code_rollback_conflicts_instead_of_restoring_snapshot_over_current_edit() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_alias_conflict");
        let root = &project.repo_root;
        fs::write(root.join("tracked.txt"), "A\n").expect("baseline");

        let group = capture_modify(&project, "edit tracked", "tracked.txt", "B\n");
        fs::write(root.join("tracked.txt"), "user overlapping edit\n").expect("human edit");

        let error = apply_code_rollback(root, &project.project_id, &group.change_group_id)
            .expect_err("rollback alias conflicts");

        assert_eq!(error.code, "code_undo_conflicted");
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read unchanged file"),
            "user overlapping edit\n"
        );

        let head = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        assert_eq!(head.workspace_epoch, 1);

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (status, failure_code): (String, String) = connection
            .query_row(
                r#"
                SELECT status, failure_code
                FROM code_rollback_operations
                WHERE project_id = ?1
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                params![project.project_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("failed operation row");
        assert_eq!(status, "failed");
        assert_eq!(failure_code, "code_undo_conflicted");
    }

    #[test]
    fn apply_code_file_undo_reverts_selected_file_and_preserves_unrelated_edit() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("undo_file_preserves_unrelated");
        let root = &project.repo_root;
        fs::write(root.join("tracked.txt"), "A\n").expect("baseline");
        fs::write(root.join("unrelated.txt"), "unrelated before\n").expect("unrelated");

        let group = capture_modify(&project, "edit tracked", "tracked.txt", "B\n");
        fs::write(root.join("unrelated.txt"), "user unrelated edit\n").expect("user unrelated");

        let undo = apply_code_file_undo(
            root,
            ApplyCodeFileUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("code-undo-test-1".into()),
                target_change_group_id: group.change_group_id.clone(),
                target_patch_file_id: None,
                target_file_path: Some("tracked.txt".into()),
                target_hunk_ids: Vec::new(),
                expected_workspace_epoch: Some(1),
            },
        )
        .expect("apply file undo");

        assert_eq!(undo.status, CodeFileUndoApplyStatus::Completed);
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read tracked"),
            "A\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("unrelated.txt")).expect("read unrelated"),
            "user unrelated edit\n"
        );
        assert_eq!(undo.conflicts, Vec::<CodeFileUndoConflict>::new());
        assert_eq!(
            undo.result_commit_id.as_deref(),
            undo.workspace_head
                .as_ref()
                .and_then(|head| head.head_id.as_deref())
        );

        let head = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        assert_eq!(head.workspace_epoch, 2);
        assert_eq!(
            head.latest_history_operation_id.as_deref(),
            Some("code-undo-test-1")
        );
        let commit = db::project_store::read_code_patchset_commit(
            root,
            &project.project_id,
            undo.result_commit_id.as_deref().expect("result commit id"),
        )
        .expect("read undo commit")
        .expect("undo commit");
        assert_eq!(commit.commit.commit_kind, CodeHistoryCommitKind::Undo);
        assert_eq!(
            commit.commit.history_operation_id.as_deref(),
            Some("code-undo-test-1")
        );
        assert_eq!(
            commit.commit.parent_commit_id,
            group
                .history_metadata
                .and_then(|metadata| metadata.commit_id)
        );
        assert_eq!(commit.patchset.file_count, 1);
        assert_eq!(commit.files[0].operation, CodePatchFileOperation::Modify);
        assert_eq!(commit.files[0].path_before.as_deref(), Some("tracked.txt"));
        assert_eq!(commit.files[0].hunks[0].removed_lines, vec!["B\n"]);
        assert_eq!(commit.files[0].hunks[0].added_lines, vec!["A\n"]);

        let path_epoch =
            db::project_store::read_code_path_epoch(root, &project.project_id, "tracked.txt")
                .expect("read tracked epoch")
                .expect("tracked epoch");
        assert_eq!(path_epoch.workspace_epoch, 2);
        assert_eq!(
            path_epoch.commit_id.as_deref(),
            undo.result_commit_id.as_deref()
        );
        let unrelated_epoch =
            db::project_store::read_code_path_epoch(root, &project.project_id, "unrelated.txt")
                .expect("read unrelated epoch");
        assert!(unrelated_epoch.is_none());

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let pending_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM code_rollback_operations WHERE project_id = ?1 AND status = 'pending'",
                params![project.project_id],
                |row| row.get(0),
            )
            .expect("pending rollback operation count");
        assert_eq!(pending_count, 0);

        let history_operations = list_code_history_operations_for_session(
            root,
            &project.project_id,
            &project.agent_session_id,
            Some(&project.run_id),
        )
        .expect("list code history operations");
        assert_eq!(history_operations.len(), 1);
        assert_eq!(history_operations[0].operation_id, "code-undo-test-1");
        assert_eq!(history_operations[0].mode, "selective_undo");
        assert_eq!(history_operations[0].status, "completed");
        assert_eq!(
            history_operations[0].target_change_group_id.as_deref(),
            Some(group.change_group_id.as_str())
        );
        assert_eq!(
            history_operations[0].result_commit_id.as_deref(),
            undo.result_commit_id.as_deref()
        );
        assert_eq!(history_operations[0].affected_paths, vec!["tracked.txt"]);
    }

    #[test]
    fn apply_code_file_undo_reports_conflict_before_writing_overlapping_edit() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("undo_file_conflict");
        let root = &project.repo_root;
        fs::write(root.join("tracked.txt"), "A\n").expect("baseline");

        let group = capture_modify(&project, "edit tracked", "tracked.txt", "B\n");
        fs::write(root.join("tracked.txt"), "human overlap\n").expect("human overlap");

        let undo = apply_code_file_undo(
            root,
            ApplyCodeFileUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("code-undo-conflict-1".into()),
                target_change_group_id: group.change_group_id.clone(),
                target_patch_file_id: None,
                target_file_path: Some("tracked.txt".into()),
                target_hunk_ids: Vec::new(),
                expected_workspace_epoch: Some(1),
            },
        )
        .expect("plan conflicted file undo");

        assert_eq!(undo.status, CodeFileUndoApplyStatus::Conflicted);
        assert_eq!(undo.result_commit_id, None);
        assert_eq!(undo.conflicts.len(), 1);
        assert_eq!(
            undo.conflicts[0].kind,
            CodeFileUndoConflictKind::TextOverlap
        );
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read tracked"),
            "human overlap\n"
        );

        let head = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        assert_eq!(head.workspace_epoch, 1);

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let result_group_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM code_change_groups WHERE project_id = ?1 AND parent_change_group_id = ?2",
                params![project.project_id, group.change_group_id],
                |row| row.get(0),
            )
            .expect("result change group count");
        assert_eq!(result_group_count, 0);
    }

    #[test]
    fn apply_code_hunk_undo_reverts_selected_hunk_and_preserves_sibling_hunks() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("undo_selected_hunk");
        let root = &project.repo_root;
        fs::write(root.join("multi.txt"), "first old\nmiddle\nthird old\n").expect("baseline");

        let group = capture_modify(
            &project,
            "edit multiple hunks",
            "multi.txt",
            "first new\nmiddle\nthird new\n",
        );
        let patch_file_id = replace_text_patch_hunks(
            &project,
            &group.change_group_id,
            "multi.txt",
            vec![
                CodePatchHunkInput {
                    hunk_id: "hunk-first".into(),
                    hunk_index: 0,
                    base_start_line: 1,
                    base_line_count: 1,
                    result_start_line: 1,
                    result_line_count: 1,
                    removed_lines: vec!["first old\n".into()],
                    added_lines: vec!["first new\n".into()],
                    context_before: Vec::new(),
                    context_after: vec!["middle\n".into()],
                },
                CodePatchHunkInput {
                    hunk_id: "hunk-third".into(),
                    hunk_index: 1,
                    base_start_line: 3,
                    base_line_count: 1,
                    result_start_line: 3,
                    result_line_count: 1,
                    removed_lines: vec!["third old\n".into()],
                    added_lines: vec!["third new\n".into()],
                    context_before: vec!["middle\n".into()],
                    context_after: Vec::new(),
                },
            ],
        );

        let undo = apply_code_file_undo(
            root,
            ApplyCodeFileUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("code-undo-hunk-1".into()),
                target_change_group_id: group.change_group_id.clone(),
                target_patch_file_id: Some(patch_file_id),
                target_file_path: Some("multi.txt".into()),
                target_hunk_ids: vec!["hunk-first".into()],
                expected_workspace_epoch: Some(1),
            },
        )
        .expect("apply hunk undo");

        assert_eq!(undo.status, CodeFileUndoApplyStatus::Completed);
        assert_eq!(undo.selected_hunk_ids, vec!["hunk-first"]);
        assert_eq!(
            fs::read_to_string(root.join("multi.txt")).expect("read multi"),
            "first old\nmiddle\nthird new\n"
        );
        let commit = db::project_store::read_code_patchset_commit(
            root,
            &project.project_id,
            undo.result_commit_id.as_deref().expect("result commit id"),
        )
        .expect("read hunk undo commit")
        .expect("hunk undo commit");
        assert_eq!(commit.commit.commit_kind, CodeHistoryCommitKind::Undo);
        assert_eq!(
            commit.commit.history_operation_id.as_deref(),
            Some("code-undo-hunk-1")
        );
        assert_eq!(commit.patchset.file_count, 1);
        assert_eq!(commit.patchset.text_hunk_count, 1);
        assert_eq!(commit.files[0].path_before.as_deref(), Some("multi.txt"));
    }

    #[test]
    fn apply_code_change_group_undo_reverts_all_files_as_one_commit() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("undo_group_full_batch");
        let root = &project.repo_root;
        fs::write(root.join("one.txt"), "one before\n").expect("one baseline");
        fs::write(root.join("two.txt"), "two before\n").expect("two baseline");
        fs::write(root.join("unrelated.txt"), "unrelated before\n").expect("unrelated");

        let handle = begin_exact_path_capture(
            root,
            project.input("edit multiple files"),
            vec![
                CodeRollbackCaptureTarget::modify("one.txt"),
                CodeRollbackCaptureTarget::modify("two.txt"),
                CodeRollbackCaptureTarget::create("created.txt"),
            ],
        )
        .expect("begin multi-file capture");
        fs::write(root.join("one.txt"), "one after\n").expect("one after");
        fs::write(root.join("two.txt"), "two after\n").expect("two after");
        fs::write(root.join("created.txt"), "created by agent\n").expect("created after");
        let group = complete_exact_path_capture(root, handle).expect("complete multi-file capture");
        fs::write(root.join("unrelated.txt"), "user unrelated edit\n").expect("user unrelated");

        let undo = apply_code_change_group_undo(
            root,
            ApplyCodeChangeGroupUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("code-undo-group-1".into()),
                target_change_group_id: group.change_group_id.clone(),
                expected_workspace_epoch: Some(1),
            },
        )
        .expect("apply change group undo");

        assert_eq!(undo.status, CodeFileUndoApplyStatus::Completed);
        assert_eq!(
            fs::read_to_string(root.join("one.txt")).expect("read one"),
            "one before\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("two.txt")).expect("read two"),
            "two before\n"
        );
        assert!(!root.join("created.txt").exists());
        assert_eq!(
            fs::read_to_string(root.join("unrelated.txt")).expect("read unrelated"),
            "user unrelated edit\n"
        );
        assert_eq!(
            undo.affected_paths,
            vec!["created.txt", "one.txt", "two.txt"]
        );

        let commit = db::project_store::read_code_patchset_commit(
            root,
            &project.project_id,
            undo.result_commit_id.as_deref().expect("result commit id"),
        )
        .expect("read undo commit")
        .expect("undo commit");
        assert_eq!(commit.commit.commit_kind, CodeHistoryCommitKind::Undo);
        assert_eq!(commit.patchset.file_count, 3);
        assert_eq!(
            commit.commit.history_operation_id.as_deref(),
            Some("code-undo-group-1")
        );
        assert_eq!(
            commit.commit.parent_commit_id,
            group
                .history_metadata
                .and_then(|metadata| metadata.commit_id)
        );

        let committed_paths = commit
            .files
            .iter()
            .flat_map(|file| [file.path_before.as_deref(), file.path_after.as_deref()])
            .flatten()
            .map(ToOwned::to_owned)
            .collect::<BTreeSet<_>>();
        assert_eq!(
            committed_paths,
            BTreeSet::from([
                "created.txt".to_string(),
                "one.txt".to_string(),
                "two.txt".to_string()
            ])
        );

        let head = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        assert_eq!(head.workspace_epoch, 2);
        assert_eq!(
            head.latest_history_operation_id.as_deref(),
            Some("code-undo-group-1")
        );
        for path in ["created.txt", "one.txt", "two.txt"] {
            let path_epoch =
                db::project_store::read_code_path_epoch(root, &project.project_id, path)
                    .expect("read path epoch")
                    .expect("path epoch");
            assert_eq!(
                path_epoch.commit_id.as_deref(),
                undo.result_commit_id.as_deref()
            );
        }
    }

    #[test]
    fn apply_code_change_group_undo_conflict_leaves_all_files_unchanged() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("undo_group_conflict_atomic");
        let root = &project.repo_root;
        fs::write(root.join("one.txt"), "one before\n").expect("one baseline");
        fs::write(root.join("two.txt"), "two before\n").expect("two baseline");

        let handle = begin_exact_path_capture(
            root,
            project.input("edit two files"),
            vec![
                CodeRollbackCaptureTarget::modify("one.txt"),
                CodeRollbackCaptureTarget::modify("two.txt"),
            ],
        )
        .expect("begin multi-file capture");
        fs::write(root.join("one.txt"), "one after\n").expect("one after");
        fs::write(root.join("two.txt"), "two after\n").expect("two after");
        let group = complete_exact_path_capture(root, handle).expect("complete multi-file capture");
        fs::write(root.join("two.txt"), "human overlap\n").expect("human overlap");

        let undo = apply_code_change_group_undo(
            root,
            ApplyCodeChangeGroupUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("code-undo-group-conflict-1".into()),
                target_change_group_id: group.change_group_id.clone(),
                expected_workspace_epoch: Some(1),
            },
        )
        .expect("plan conflicted change group undo");

        assert_eq!(undo.status, CodeFileUndoApplyStatus::Conflicted);
        assert_eq!(undo.result_commit_id, None);
        assert_eq!(undo.conflicts.len(), 1);
        assert_eq!(undo.conflicts[0].path, "two.txt");
        assert_eq!(
            fs::read_to_string(root.join("one.txt")).expect("read one"),
            "one after\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("two.txt")).expect("read two"),
            "human overlap\n"
        );

        let head = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        assert_eq!(head.workspace_epoch, 1);

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let result_group_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM code_change_groups WHERE project_id = ?1 AND parent_change_group_id = ?2",
                params![project.project_id, group.change_group_id],
                |row| row.get(0),
            )
            .expect("result change group count");
        assert_eq!(result_group_count, 0);
    }

    #[test]
    fn completed_undo_delivers_mailbox_notices_to_affected_and_project_runs() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("undo_mailbox_completed");
        let root = &project.repo_root;
        fs::write(root.join("shared.txt"), "base\n").expect("shared baseline");
        let group = capture_modify(&project, "shared edit", "shared.txt", "after\n");

        let reservation_session = create_test_agent_session(&project, "Reservation run");
        let reservation_run = "run-mailbox-reservation";
        insert_test_agent_run(
            &project,
            &reservation_session,
            reservation_run,
            "2026-05-06T12:00:03Z",
        );
        activate_test_run(
            &project,
            reservation_run,
            "Active reservation over shared.txt.",
        );
        claim_test_reservation(&project, reservation_run, "shared.txt");

        let recent_session = create_test_agent_session(&project, "Recent activity run");
        let recent_run = "run-mailbox-recent";
        insert_test_agent_run(
            &project,
            &recent_session,
            recent_run,
            "2026-05-06T12:00:04Z",
        );
        activate_test_run(&project, recent_run, "Recently touched shared.txt.");
        append_test_recent_path_activity(&project, recent_run, "shared.txt");

        let other_session = create_test_agent_session(&project, "Other active run");
        let other_run = "run-mailbox-other";
        insert_test_agent_run(&project, &other_session, other_run, "2026-05-06T12:00:05Z");
        activate_test_run(&project, other_run, "Working elsewhere.");

        let undo = apply_code_file_undo(
            root,
            ApplyCodeFileUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("code-undo-mailbox-completed-1".into()),
                target_change_group_id: group.change_group_id.clone(),
                target_patch_file_id: None,
                target_file_path: Some("shared.txt".into()),
                target_hunk_ids: Vec::new(),
                expected_workspace_epoch: Some(1),
            },
        )
        .expect("apply file undo");

        assert_eq!(undo.status, CodeFileUndoApplyStatus::Completed);

        let reservation_inbox = db::project_store::list_agent_mailbox_inbox(
            root,
            &project.project_id,
            reservation_run,
            &now_timestamp(),
            10,
        )
        .expect("reservation inbox");
        assert_eq!(reservation_inbox.len(), 1);
        assert_eq!(
            reservation_inbox[0].item.item_type,
            db::project_store::AgentMailboxItemType::HistoryRewriteNotice
        );
        assert_eq!(
            reservation_inbox[0].item.priority,
            db::project_store::AgentMailboxPriority::High
        );
        assert_eq!(
            reservation_inbox[0].item.related_paths,
            vec!["shared.txt".to_string()]
        );
        assert_eq!(
            reservation_inbox[0].item.target_run_id.as_deref(),
            Some(reservation_run)
        );

        let recent_inbox = db::project_store::list_agent_mailbox_inbox(
            root,
            &project.project_id,
            recent_run,
            &now_timestamp(),
            10,
        )
        .expect("recent inbox");
        assert_eq!(recent_inbox.len(), 1);
        assert_eq!(
            recent_inbox[0].item.item_type,
            db::project_store::AgentMailboxItemType::HistoryRewriteNotice
        );
        assert_eq!(
            recent_inbox[0].item.priority,
            db::project_store::AgentMailboxPriority::High
        );

        let other_inbox = db::project_store::list_agent_mailbox_inbox(
            root,
            &project.project_id,
            other_run,
            &now_timestamp(),
            10,
        )
        .expect("other inbox");
        assert_eq!(other_inbox.len(), 1);
        assert_eq!(
            other_inbox[0].item.item_type,
            db::project_store::AgentMailboxItemType::WorkspaceEpochAdvanced
        );
        assert_eq!(
            other_inbox[0].item.priority,
            db::project_store::AgentMailboxPriority::Normal
        );
    }

    #[test]
    fn conflicted_undo_delivers_high_mailbox_notice_only_to_affected_runs() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("undo_mailbox_conflict");
        let root = &project.repo_root;
        fs::write(root.join("shared.txt"), "base\n").expect("shared baseline");
        let group = capture_modify(&project, "shared edit", "shared.txt", "after\n");
        fs::write(root.join("shared.txt"), "human overlap\n").expect("overlap");

        let reservation_session = create_test_agent_session(&project, "Reservation run");
        let reservation_run = "run-mailbox-conflict-reservation";
        insert_test_agent_run(
            &project,
            &reservation_session,
            reservation_run,
            "2026-05-06T12:00:03Z",
        );
        activate_test_run(
            &project,
            reservation_run,
            "Active reservation over shared.txt.",
        );
        claim_test_reservation(&project, reservation_run, "shared.txt");

        let other_session = create_test_agent_session(&project, "Other active run");
        let other_run = "run-mailbox-conflict-other";
        insert_test_agent_run(&project, &other_session, other_run, "2026-05-06T12:00:04Z");
        activate_test_run(&project, other_run, "Working elsewhere.");

        let undo = apply_code_file_undo(
            root,
            ApplyCodeFileUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("code-undo-mailbox-conflict-1".into()),
                target_change_group_id: group.change_group_id.clone(),
                target_patch_file_id: None,
                target_file_path: Some("shared.txt".into()),
                target_hunk_ids: Vec::new(),
                expected_workspace_epoch: Some(1),
            },
        )
        .expect("plan file undo conflict");

        assert_eq!(undo.status, CodeFileUndoApplyStatus::Conflicted);

        let reservation_inbox = db::project_store::list_agent_mailbox_inbox(
            root,
            &project.project_id,
            reservation_run,
            &now_timestamp(),
            10,
        )
        .expect("reservation inbox");
        assert_eq!(reservation_inbox.len(), 1);
        assert_eq!(
            reservation_inbox[0].item.item_type,
            db::project_store::AgentMailboxItemType::UndoConflictNotice
        );
        assert_eq!(
            reservation_inbox[0].item.priority,
            db::project_store::AgentMailboxPriority::High
        );

        let other_inbox = db::project_store::list_agent_mailbox_inbox(
            root,
            &project.project_id,
            other_run,
            &now_timestamp(),
            10,
        )
        .expect("other inbox");
        assert!(other_inbox.is_empty());
    }

    #[test]
    fn apply_code_session_rollback_batch_preserves_later_sibling_session_changes() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("session_rollback_preserves_siblings");
        let root = &project.repo_root;
        fs::write(root.join("boundary.txt"), "boundary before\n").expect("boundary baseline");
        let boundary_group = capture_modify(
            &project,
            "boundary edit",
            "boundary.txt",
            "boundary after\n",
        );

        fs::write(root.join("a.txt"), "A0\n").expect("a baseline");
        let a_first = capture_modify(&project, "session A first edit", "a.txt", "A1\n");

        let session_b = create_test_agent_session(&project, "Session B");
        let run_b = "run-session-b";
        insert_test_agent_run(&project, &session_b, run_b, "2026-05-06T12:00:03Z");
        fs::write(root.join("b.txt"), "B0\n").expect("b baseline");
        let _b_group = capture_modify_for_session_run(
            &project,
            &session_b,
            run_b,
            "session B edit",
            "b.txt",
            "B1\n",
        );

        let session_c = create_test_agent_session(&project, "Session C");
        let run_c = "run-session-c";
        insert_test_agent_run(&project, &session_c, run_c, "2026-05-06T12:00:04Z");
        fs::write(root.join("c.txt"), "C0\n").expect("c baseline");
        let _c_group = capture_modify_for_session_run(
            &project,
            &session_c,
            run_c,
            "session C edit",
            "c.txt",
            "C1\n",
        );

        let a_second = capture_modify(&project, "session A second edit", "a.txt", "A2\n");
        let head_before = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");

        let rollback = apply_code_session_rollback(
            root,
            ApplyCodeSessionRollbackRequest {
                boundary: ResolveCodeSessionBoundaryRequest {
                    project_id: project.project_id.clone(),
                    agent_session_id: project.agent_session_id.clone(),
                    target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                    target_id: format!("change_group:{}", boundary_group.change_group_id),
                    boundary_id: format!("change_group:{}", boundary_group.change_group_id),
                    run_id: None,
                    change_group_id: Some(boundary_group.change_group_id.clone()),
                },
                operation_id: Some("code-session-rollback-preserve-1".into()),
                explicitly_selected_change_group_ids: Vec::new(),
                expected_workspace_epoch: Some(head_before.workspace_epoch),
            },
        )
        .expect("apply session rollback");

        assert_eq!(rollback.status, CodeFileUndoApplyStatus::Completed);
        assert_eq!(
            rollback.target_change_group_ids,
            vec![
                a_second.change_group_id.clone(),
                a_first.change_group_id.clone()
            ]
        );
        assert_eq!(
            fs::read_to_string(root.join("a.txt")).expect("read a"),
            "A0\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("b.txt")).expect("read b"),
            "B1\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("c.txt")).expect("read c"),
            "C1\n"
        );
        assert_eq!(rollback.affected_paths, vec!["a.txt"]);
        assert_eq!(rollback.affected_files.len(), 1);

        let commit = db::project_store::read_code_patchset_commit(
            root,
            &project.project_id,
            rollback
                .result_commit_id
                .as_deref()
                .expect("result commit id"),
        )
        .expect("read session rollback commit")
        .expect("session rollback commit");
        assert_eq!(
            commit.commit.commit_kind,
            CodeHistoryCommitKind::SessionRollback
        );
        assert_eq!(
            commit.commit.history_operation_id.as_deref(),
            Some("code-session-rollback-preserve-1")
        );
        assert_eq!(commit.commit.parent_commit_id, head_before.head_id);
        assert_eq!(commit.patchset.file_count, 1);
        assert_eq!(commit.files[0].path_before.as_deref(), Some("a.txt"));
        assert_eq!(commit.files[0].path_after.as_deref(), Some("a.txt"));

        let head_after = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        assert_eq!(head_after.workspace_epoch, head_before.workspace_epoch + 1);
        assert_eq!(
            head_after.latest_history_operation_id.as_deref(),
            Some("code-session-rollback-preserve-1")
        );

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (change_kind, status, parent_change_group_id): (String, String, String) = connection
            .query_row(
                r#"
                SELECT change_kind, status, parent_change_group_id
                FROM code_change_groups
                WHERE project_id = ?1
                  AND change_group_id = ?2
                "#,
                params![
                    project.project_id,
                    rollback
                        .result_change_group_id
                        .as_deref()
                        .expect("result change group id")
                ],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("session rollback change group");
        assert_eq!(change_kind, "rollback");
        assert_eq!(status, "completed");
        assert_eq!(parent_change_group_id, boundary_group.change_group_id);

        let history_operations = list_code_history_operations_for_session(
            root,
            &project.project_id,
            &project.agent_session_id,
            Some(&project.run_id),
        )
        .expect("list session rollback history operations");
        assert_eq!(history_operations.len(), 1);
        assert_eq!(
            history_operations[0].operation_id,
            "code-session-rollback-preserve-1"
        );
        assert_eq!(history_operations[0].mode, "session_rollback");
        assert_eq!(history_operations[0].status, "completed");
        assert_eq!(
            history_operations[0].target_change_group_id.as_deref(),
            Some(boundary_group.change_group_id.as_str())
        );
        assert_eq!(
            history_operations[0].result_commit_id.as_deref(),
            rollback.result_commit_id.as_deref()
        );
        assert_eq!(history_operations[0].affected_paths, vec!["a.txt"]);
    }

    #[test]
    fn apply_code_session_rollback_batch_conflicts_when_sibling_changed_same_lines() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("session_rollback_sibling_conflict");
        let root = &project.repo_root;
        fs::write(root.join("boundary.txt"), "boundary before\n").expect("boundary baseline");
        let boundary_group = capture_modify(
            &project,
            "boundary edit",
            "boundary.txt",
            "boundary after\n",
        );

        fs::write(root.join("shared.txt"), "base\n").expect("shared baseline");
        let a_group = capture_modify(&project, "session A shared edit", "shared.txt", "A owns\n");

        let session_b = create_test_agent_session(&project, "Session B");
        let run_b = "run-session-b-conflict";
        insert_test_agent_run(&project, &session_b, run_b, "2026-05-06T12:00:03Z");
        let _b_group = capture_modify_for_session_run(
            &project,
            &session_b,
            run_b,
            "session B shared edit",
            "shared.txt",
            "B owns\n",
        );

        let session_c = create_test_agent_session(&project, "Session C");
        let run_c = "run-session-c-conflict";
        insert_test_agent_run(&project, &session_c, run_c, "2026-05-06T12:00:04Z");
        fs::write(root.join("c.txt"), "C0\n").expect("c baseline");
        let _c_group = capture_modify_for_session_run(
            &project,
            &session_c,
            run_c,
            "session C edit",
            "c.txt",
            "C1\n",
        );

        let head_before = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        let rollback = apply_code_session_rollback(
            root,
            ApplyCodeSessionRollbackRequest {
                boundary: ResolveCodeSessionBoundaryRequest {
                    project_id: project.project_id.clone(),
                    agent_session_id: project.agent_session_id.clone(),
                    target_kind: CodeSessionBoundaryTargetKind::SessionBoundary,
                    target_id: format!("change_group:{}", boundary_group.change_group_id),
                    boundary_id: format!("change_group:{}", boundary_group.change_group_id),
                    run_id: None,
                    change_group_id: Some(boundary_group.change_group_id.clone()),
                },
                operation_id: Some("code-session-rollback-conflict-1".into()),
                explicitly_selected_change_group_ids: Vec::new(),
                expected_workspace_epoch: Some(head_before.workspace_epoch),
            },
        )
        .expect("plan conflicted session rollback");

        assert_eq!(rollback.status, CodeFileUndoApplyStatus::Conflicted);
        assert_eq!(
            rollback.target_change_group_ids,
            vec![a_group.change_group_id]
        );
        assert_eq!(rollback.result_commit_id, None);
        assert_eq!(rollback.result_change_group_id, None);
        assert_eq!(rollback.conflicts.len(), 1);
        assert_eq!(rollback.conflicts[0].path, "shared.txt");
        assert_eq!(
            rollback.conflicts[0].kind,
            CodeFileUndoConflictKind::TextOverlap
        );
        assert_eq!(
            fs::read_to_string(root.join("shared.txt")).expect("read shared"),
            "B owns\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("c.txt")).expect("read c"),
            "C1\n"
        );

        let head_after = db::project_store::read_code_workspace_head(root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head");
        assert_eq!(head_after.workspace_epoch, head_before.workspace_epoch);
        assert_eq!(head_after.head_id, head_before.head_id);

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let result_group_count: i64 = connection
            .query_row(
                "SELECT COUNT(*) FROM code_change_groups WHERE project_id = ?1 AND parent_change_group_id = ?2",
                params![project.project_id, boundary_group.change_group_id],
                |row| row.get(0),
            )
            .expect("result change group count");
        assert_eq!(result_group_count, 0);
    }

    #[test]
    fn retention_prunes_only_unreachable_blobs_and_preserves_rollback_snapshots() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_retention");
        let root = &project.repo_root;
        fs::write(root.join("tracked.txt"), "A\n").expect("baseline");
        fs::write(root.join("unrelated.txt"), "unrelated before\n").expect("unrelated");
        let _group_1 = capture_modify(&project, "edit 1", "tracked.txt", "B\n");
        let group_2 = capture_modify(&project, "edit 2", "tracked.txt", "C\n");
        fs::write(root.join("unrelated.txt"), "human edit\n").expect("human edit");
        let rollback = apply_code_rollback(root, &project.project_id, &group_2.change_group_id)
            .expect("apply rollback");
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read undone tracked"),
            "B\n"
        );
        let orphan_blob_id = "a".repeat(64);
        let orphan_blob_path = insert_orphan_blob(&project, &orphan_blob_id);

        let report = prune_code_rollback_blobs(
            root,
            &project.project_id,
            CodeRollbackRetentionPolicy {
                min_unreferenced_age_seconds: 0,
            },
        )
        .expect("prune blobs");

        assert_eq!(report.pruned_blob_count, 1);
        assert!(!orphan_blob_path.exists());
        let undo = apply_code_rollback(root, &project.project_id, &rollback.result_change_group_id)
            .expect("rollback the rollback after prune");
        assert_eq!(undo.target_snapshot_id, rollback.pre_rollback_snapshot_id);
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read restored target edit"),
            "C\n"
        );
        assert_eq!(
            fs::read_to_string(root.join("unrelated.txt")).expect("read preserved human edit"),
            "human edit\n"
        );
    }

    #[test]
    fn retention_preserves_patch_history_and_conflict_diagnostic_blobs() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("patch_history_retention");
        let root = &project.repo_root;
        fs::write(root.join("tracked.txt"), "before\n").expect("baseline");
        let group = capture_modify(&project, "edit tracked", "tracked.txt", "after\n");
        let (patch_base_blob_id, patch_base_blob_path) =
            insert_test_blob(&project, b"patch-history-retention-before");
        let (patch_result_blob_id, patch_result_blob_path) =
            insert_test_blob(&project, b"patch-history-retention-after");
        let (conflict_blob_id, conflict_blob_path) =
            insert_test_blob(&project, b"patch-history-retention-conflict-current");
        let orphan_blob_id = "d".repeat(64);
        let orphan_blob_path = insert_orphan_blob(&project, &orphan_blob_id);
        let undo_operation_id = "history-op-retention-undo".to_string();

        match begin_code_history_operation(
            root,
            &CodeHistoryOperationStart {
                project_id: project.project_id.clone(),
                operation_id: undo_operation_id.clone(),
                mode: CodeHistoryOperationMode::SelectiveUndo,
                target_kind: CodeHistoryOperationTargetKind::ChangeGroup,
                target_id: group.change_group_id.clone(),
                target_change_group_id: Some(group.change_group_id.clone()),
                target_file_path: None,
                target_hunk_ids: Vec::new(),
                agent_session_id: Some(project.agent_session_id.clone()),
                run_id: Some(project.run_id.clone()),
                expected_workspace_epoch: None,
                affected_paths: vec!["patch-only.bin".into()],
            },
        )
        .expect("begin synthetic undo operation")
        {
            CodeHistoryOperationBegin::Started => {}
            CodeHistoryOperationBegin::Existing(_) => panic!("unexpected existing undo operation"),
        }
        persist_code_patchset_commit(
            root,
            &CodePatchsetCommitInput {
                project_id: project.project_id.clone(),
                commit_id: "code-commit-retention-undo".into(),
                parent_commit_id: None,
                tree_id: "code-tree-retention-after".into(),
                parent_tree_id: Some("code-tree-retention-before".into()),
                patchset_id: "code-patchset-retention-undo".into(),
                change_group_id: group.change_group_id.clone(),
                history_operation_id: Some(undo_operation_id.clone()),
                agent_session_id: project.agent_session_id.clone(),
                run_id: project.run_id.clone(),
                tool_call_id: None,
                runtime_event_id: None,
                conversation_sequence: None,
                commit_kind: CodeHistoryCommitKind::Undo,
                summary_label: "synthetic undo retention".into(),
                workspace_epoch: 20,
                created_at: "2026-05-06T12:00:00Z".into(),
                completed_at: "2026-05-06T12:00:01Z".into(),
                files: vec![CodePatchFileInput {
                    patch_file_id: "patch-file-retention-undo".into(),
                    path_before: Some("patch-only.bin".into()),
                    path_after: Some("patch-only.bin".into()),
                    operation: CodePatchFileOperation::Modify,
                    merge_policy: CodePatchMergePolicy::Exact,
                    before_file_kind: Some(CodePatchFileKind::File),
                    after_file_kind: Some(CodePatchFileKind::File),
                    base_hash: Some(patch_base_blob_id.clone()),
                    result_hash: Some(patch_result_blob_id.clone()),
                    base_blob_id: Some(patch_base_blob_id.clone()),
                    result_blob_id: Some(patch_result_blob_id.clone()),
                    base_size: Some("patch-history-retention-before".len() as u64),
                    result_size: Some("patch-history-retention-after".len() as u64),
                    base_mode: Some(0o644),
                    result_mode: Some(0o644),
                    base_symlink_target: None,
                    result_symlink_target: None,
                    hunks: Vec::new(),
                }],
            },
        )
        .expect("persist synthetic undo patchset");
        mark_code_history_operation_completed(
            root,
            &project.project_id,
            &undo_operation_id,
            None,
            Some("code-commit-retention-undo"),
            &["patch-only.bin".into()],
        )
        .expect("complete synthetic undo operation");

        let conflict_operation_id = "history-op-retention-conflict";
        match begin_code_history_operation(
            root,
            &CodeHistoryOperationStart {
                project_id: project.project_id.clone(),
                operation_id: conflict_operation_id.into(),
                mode: CodeHistoryOperationMode::SelectiveUndo,
                target_kind: CodeHistoryOperationTargetKind::FileChange,
                target_id: "patch-file-retention-conflict".into(),
                target_change_group_id: Some(group.change_group_id.clone()),
                target_file_path: Some("conflict.bin".into()),
                target_hunk_ids: Vec::new(),
                agent_session_id: Some(project.agent_session_id.clone()),
                run_id: Some(project.run_id.clone()),
                expected_workspace_epoch: None,
                affected_paths: vec!["conflict.bin".into()],
            },
        )
        .expect("begin synthetic conflict operation")
        {
            CodeHistoryOperationBegin::Started => {}
            CodeHistoryOperationBegin::Existing(_) => {
                panic!("unexpected existing conflict operation")
            }
        }
        mark_code_history_operation_conflicted(
            root,
            &project.project_id,
            conflict_operation_id,
            &["conflict.bin".into()],
            &[CodeFileUndoConflict {
                path: "conflict.bin".into(),
                kind: CodeFileUndoConflictKind::ContentMismatch,
                message: "Synthetic conflict diagnostic retains its current blob.".into(),
                base_hash: None,
                selected_hash: None,
                current_hash: Some(conflict_blob_id.clone()),
                hunk_ids: Vec::new(),
            }],
        )
        .expect("mark synthetic conflict operation");

        let report = prune_code_rollback_blobs(
            root,
            &project.project_id,
            CodeRollbackRetentionPolicy {
                min_unreferenced_age_seconds: 0,
            },
        )
        .expect("prune blobs");

        assert_eq!(report.pruned_blob_count, 1);
        assert_eq!(report.retained_unreferenced_blob_count, 0);
        assert!(report.diagnostics.is_empty());
        assert!(!orphan_blob_path.exists());
        assert!(patch_base_blob_path.exists());
        assert!(patch_result_blob_path.exists());
        assert!(conflict_blob_path.exists());

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let retained_count: i64 = connection
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM code_blobs
                WHERE project_id = ?1
                  AND blob_id IN (?2, ?3, ?4)
                "#,
                params![
                    project.project_id,
                    patch_base_blob_id,
                    patch_result_blob_id,
                    conflict_blob_id,
                ],
                |row| row.get(0),
            )
            .expect("retained blob count");
        assert_eq!(retained_count, 3);
    }

    #[test]
    fn missing_blob_fails_restore_before_changing_project_files() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_missing_blob");
        let root = &project.repo_root;
        fs::write(root.join("file.txt"), "before\n").expect("before");

        let handle = begin_exact_path_capture(
            root,
            project.input("missing blob"),
            vec![CodeRollbackCaptureTarget::modify("file.txt")],
        )
        .expect("begin");
        fs::write(root.join("file.txt"), "after\n").expect("after");
        let completed = complete_exact_path_capture(root, handle).expect("complete");

        let (connection, database_path) = open_code_rollback_database(root).expect("db");
        let manifest = load_completed_snapshot_manifest(
            &connection,
            &database_path,
            &project.project_id,
            &completed.before_snapshot_id,
        )
        .expect("manifest");
        let blob_id = manifest.entries[0].blob_id.as_deref().expect("blob id");
        let blob_path = project_app_data_dir_for_repo(root).join(blob_relative_path(blob_id));
        fs::remove_file(blob_path).expect("remove blob");

        let error = restore_code_snapshot(root, &project.project_id, &completed.before_snapshot_id)
            .expect_err("restore fails");

        assert_eq!(error.code, "code_rollback_blob_missing");
        assert_eq!(
            fs::read_to_string(root.join("file.txt")).expect("read file"),
            "after\n"
        );
    }

    #[test]
    fn failed_apply_code_rollback_records_audit_without_success_state() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_apply_missing_blob");
        let root = &project.repo_root;
        fs::write(root.join("file.txt"), "before\n").expect("before");
        let handle = begin_exact_path_capture(
            root,
            project.input("missing target blob"),
            vec![CodeRollbackCaptureTarget::delete("file.txt")],
        )
        .expect("begin delete capture");
        fs::remove_file(root.join("file.txt")).expect("delete file");
        let completed = complete_exact_path_capture(root, handle).expect("complete delete");

        let (connection, database_path) = open_code_rollback_database(root).expect("db");
        let manifest = load_completed_snapshot_manifest(
            &connection,
            &database_path,
            &project.project_id,
            &completed.before_snapshot_id,
        )
        .expect("manifest");
        let blob_id = manifest.entries[0].blob_id.as_deref().expect("blob id");
        let blob_path = project_app_data_dir_for_repo(root).join(blob_relative_path(blob_id));
        fs::remove_file(blob_path).expect("remove blob");
        drop(connection);

        let error = apply_code_rollback(root, &project.project_id, &completed.change_group_id)
            .expect_err("apply rollback fails");

        assert_eq!(error.code, "code_rollback_blob_missing");
        assert!(!root.join("file.txt").exists());

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (status, failure_code, pre_snapshot_id, result_change_group_id): (
            String,
            String,
            Option<String>,
            Option<String>,
        ) = connection
            .query_row(
                r#"
                SELECT status, failure_code, pre_rollback_snapshot_id, result_change_group_id
                FROM code_rollback_operations
                WHERE project_id = ?1
                ORDER BY created_at DESC
                LIMIT 1
                "#,
                params![project.project_id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .expect("failed operation row");
        assert_eq!(status, "failed");
        assert_eq!(failure_code, "code_rollback_blob_missing");
        assert!(pre_snapshot_id.is_none());
        assert!(result_change_group_id.is_none());
    }

    #[test]
    fn startup_validation_marks_missing_blob_snapshots_failed() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_validate_missing_blob");
        let root = &project.repo_root;
        fs::write(root.join("file.txt"), "before\n").expect("before");
        let completed = capture_modify(&project, "missing blob diagnostic", "file.txt", "after\n");
        let (connection, database_path) = open_code_rollback_database(root).expect("db");
        let manifest = load_completed_snapshot_manifest(
            &connection,
            &database_path,
            &project.project_id,
            &completed.before_snapshot_id,
        )
        .expect("manifest");
        let blob_id = manifest.entries[0].blob_id.as_deref().expect("blob id");
        let blob_path = project_app_data_dir_for_repo(root).join(blob_relative_path(blob_id));
        fs::remove_file(blob_path).expect("remove blob");
        drop(connection);

        let diagnostics =
            validate_code_rollback_storage(root, &project.project_id).expect("validate");

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "code_rollback_blob_missing"));
        let (connection, _database_path) = open_code_rollback_database(root).expect("db");
        let state: String = connection
            .query_row(
                "SELECT write_state FROM code_snapshots WHERE project_id = ?1 AND snapshot_id = ?2",
                params![project.project_id, completed.before_snapshot_id],
                |row| row.get(0),
            )
            .expect("snapshot state");
        assert_eq!(state, "failed");
    }

    #[test]
    fn startup_validation_reports_moved_project_roots_without_disabling_snapshot() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_root_moved");
        let root = &project.repo_root;
        fs::write(root.join("file.txt"), "before\n").expect("before");
        let completed = capture_modify(&project, "moved root", "file.txt", "after\n");
        rewrite_snapshot_root(&project, &completed.before_snapshot_id, "/old/xero/root");

        let diagnostics =
            validate_code_rollback_storage(root, &project.project_id).expect("validate");

        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.code == "code_snapshot_root_moved"
                && diagnostic.snapshot_id.as_deref() == Some(completed.before_snapshot_id.as_str())
        }));
        let (connection, _database_path) = open_code_rollback_database(root).expect("db");
        let state: String = connection
            .query_row(
                "SELECT write_state FROM code_snapshots WHERE project_id = ?1 AND snapshot_id = ?2",
                params![project.project_id, completed.before_snapshot_id],
                |row| row.get(0),
            )
            .expect("snapshot state");
        assert_eq!(state, "completed");
    }

    #[test]
    fn startup_validation_marks_incomplete_snapshot_writes_failed() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_startup_validation");
        let root = &project.repo_root;
        let (connection, _database_path) = open_code_rollback_database(root).expect("db");
        let request = SnapshotCaptureRequest {
            project_id: project.project_id.clone(),
            agent_session_id: project.agent_session_id.clone(),
            run_id: project.run_id.clone(),
            change_group_id: None,
            boundary_kind: CodeSnapshotBoundaryKind::Before,
            previous_snapshot_id: None,
            explicit_paths: BTreeSet::new(),
        };
        insert_pending_snapshot(&connection, root, &request, "code-snapshot-pending")
            .expect("pending snapshot");
        drop(connection);

        let diagnostics =
            validate_code_rollback_storage(root, &project.project_id).expect("validate");

        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, "code_snapshot_write_incomplete");
        let (connection, _database_path) = open_code_rollback_database(root).expect("db");
        let state: String = connection
            .query_row(
                "SELECT write_state FROM code_snapshots WHERE project_id = ?1 AND snapshot_id = 'code-snapshot-pending'",
                params![project.project_id],
                |row| row.get(0),
            )
            .expect("state");
        assert_eq!(state, "failed");
    }

    #[test]
    fn startup_validation_marks_interrupted_rollback_operations_failed() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_interrupted_operation");
        let root = &project.repo_root;
        fs::write(root.join("file.txt"), "before\n").expect("before");
        let target = capture_modify(&project, "target", "file.txt", "after\n");
        let result_change_group_id = "code-change-interrupted-rollback";
        insert_change_group_open(
            root,
            &CodeChangeGroupInput {
                change_group_id: Some(result_change_group_id.into()),
                parent_change_group_id: Some(target.change_group_id.clone()),
                change_kind: CodeChangeKind::Rollback,
                summary_label: "interrupted rollback".into(),
                ..project.input("interrupted rollback")
            },
            result_change_group_id,
        )
        .expect("insert rollback change group");
        insert_pending_rollback_operation(
            root,
            "code-rollback-interrupted",
            &RollbackTargetChangeGroup {
                project_id: project.project_id.clone(),
                agent_session_id: project.agent_session_id.clone(),
                run_id: project.run_id.clone(),
                change_group_id: target.change_group_id,
                before_snapshot_id: target.before_snapshot_id,
                summary_label: "target".into(),
            },
            Some(result_change_group_id),
        )
        .expect("insert pending operation");

        let diagnostics =
            validate_code_rollback_storage(root, &project.project_id).expect("validate storage");

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "code_rollback_operation_incomplete"));
        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (operation_status, failure_code): (String, String) = connection
            .query_row(
                "SELECT status, failure_code FROM code_rollback_operations WHERE project_id = ?1 AND operation_id = 'code-rollback-interrupted'",
                params![project.project_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("operation status");
        assert_eq!(operation_status, "failed");
        assert_eq!(failure_code, "code_rollback_operation_incomplete");
    }

    #[test]
    fn startup_recovery_marks_planning_history_operations_failed() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("history_operation_planning_recovery");
        let root = &project.repo_root;
        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        connection
            .execute(
                r#"
                INSERT INTO code_history_operations (
                    project_id,
                    operation_id,
                    mode,
                    status,
                    target_kind,
                    target_id,
                    affected_paths_json,
                    conflicts_json,
                    created_at,
                    updated_at
                )
                VALUES (?1, 'history-op-planning', 'selective_undo', 'planning',
                        'change_group', 'code-change-missing', '["file.txt"]', '[]',
                        '2026-05-06T12:00:00Z', '2026-05-06T12:00:00Z')
                "#,
                params![project.project_id],
            )
            .expect("insert planning operation");
        drop(connection);

        let diagnostics =
            validate_code_rollback_storage(root, &project.project_id).expect("validate storage");

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "code_history_operation_interrupted"));
        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (status, failure_code): (String, String) = connection
            .query_row(
                r#"
                SELECT status, failure_code
                FROM code_history_operations
                WHERE project_id = ?1
                  AND operation_id = 'history-op-planning'
                "#,
                params![project.project_id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("operation status");
        assert_eq!(status, "failed");
        assert_eq!(failure_code, "code_history_operation_interrupted");
    }

    #[test]
    fn startup_recovery_marks_applying_history_operations_repair_needed() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("history_operation_applying_recovery");
        let root = &project.repo_root;
        fs::write(root.join("file.txt"), "before\n").expect("before");
        let target = capture_modify(&project, "target", "file.txt", "after\n");
        let result_change_group_id = "code-change-history-applying";
        insert_change_group_open(
            root,
            &CodeChangeGroupInput {
                change_group_id: Some(result_change_group_id.into()),
                parent_change_group_id: Some(target.change_group_id.clone()),
                change_kind: CodeChangeKind::Rollback,
                summary_label: "interrupted history operation".into(),
                ..project.input("interrupted history operation")
            },
            result_change_group_id,
        )
        .expect("insert result change group");
        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        connection
            .execute(
                r#"
                INSERT INTO code_history_operations (
                    project_id,
                    operation_id,
                    mode,
                    status,
                    target_kind,
                    target_id,
                    target_change_group_id,
                    affected_paths_json,
                    conflicts_json,
                    result_change_group_id,
                    created_at,
                    updated_at
                )
                VALUES (?1, 'history-op-applying', 'selective_undo', 'applying',
                        'change_group', ?2, ?2, '["file.txt"]', '[]', ?3,
                        '2026-05-06T12:00:00Z', '2026-05-06T12:00:01Z')
                "#,
                params![
                    project.project_id,
                    target.change_group_id,
                    result_change_group_id
                ],
            )
            .expect("insert applying operation");
        drop(connection);

        let diagnostics =
            validate_code_rollback_storage(root, &project.project_id).expect("validate storage");

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "code_history_operation_apply_interrupted"));
        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (operation_status, repair_code, change_group_status): (String, String, String) =
            connection
                .query_row(
                    r#"
                    SELECT operation.status, operation.repair_code, change_group.status
                    FROM code_history_operations operation
                    JOIN code_change_groups change_group
                      ON change_group.project_id = operation.project_id
                     AND change_group.change_group_id = operation.result_change_group_id
                    WHERE operation.project_id = ?1
                      AND operation.operation_id = 'history-op-applying'
                    "#,
                    params![project.project_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )
                .expect("operation status");
        assert_eq!(operation_status, "repair_needed");
        assert_eq!(repair_code, "code_history_operation_apply_interrupted");
        assert_eq!(change_group_status, "failed");
    }

    #[test]
    fn startup_recovery_completes_interrupted_history_operation_without_duplicate_commit() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("history_operation_completed_recovery");
        let root = &project.repo_root;
        fs::write(root.join("file.txt"), "before\n").expect("before");
        let target = capture_modify(&project, "target", "file.txt", "after\n");

        let undo = apply_code_change_group_undo(
            root,
            ApplyCodeChangeGroupUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("history-op-completed".into()),
                target_change_group_id: target.change_group_id.clone(),
                expected_workspace_epoch: Some(1),
            },
        )
        .expect("apply undo");
        let result_change_group_id = undo
            .result_change_group_id
            .clone()
            .expect("result change group id");
        let expected_result_commit_id = undo.result_commit_id.clone().expect("undo result commit");
        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        connection
            .execute(
                r#"
                UPDATE code_history_operations
                SET status = 'applying',
                    result_commit_id = NULL,
                    completed_at = NULL,
                    updated_at = '2026-05-06T12:00:02Z'
                WHERE project_id = ?1
                  AND operation_id = 'history-op-completed'
                "#,
                params![project.project_id],
            )
            .expect("simulate interrupted terminal update");
        drop(connection);

        let diagnostics =
            validate_code_rollback_storage(root, &project.project_id).expect("validate storage");

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "code_history_operation_recovered_completed"));
        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (status, result_commit_id, affected_count, commit_count): (String, String, i64, i64) =
            connection
                .query_row(
                    r#"
                    SELECT operation.status,
                           operation.result_commit_id,
                           json_array_length(operation.affected_paths_json),
                           (
                               SELECT COUNT(*)
                               FROM code_commits code_commit
                               WHERE code_commit.project_id = operation.project_id
                                 AND code_commit.history_operation_id = operation.operation_id
                           )
                    FROM code_history_operations operation
                    WHERE operation.project_id = ?1
                      AND operation.operation_id = 'history-op-completed'
                    "#,
                    params![project.project_id],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )
                .expect("operation recovery status");
        assert_eq!(status, "completed");
        assert_eq!(result_commit_id, expected_result_commit_id);
        assert_eq!(affected_count, 1);
        assert_eq!(commit_count, 1);
        drop(connection);

        let retry = apply_code_change_group_undo(
            root,
            ApplyCodeChangeGroupUndoRequest {
                project_id: project.project_id.clone(),
                operation_id: Some("history-op-completed".into()),
                target_change_group_id: result_change_group_id,
                expected_workspace_epoch: None,
            },
        )
        .expect_err("resolved operation id cannot be reused");
        assert_eq!(retry.code, "code_history_operation_already_resolved");
        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let duplicate_count: i64 = connection
            .query_row(
                r#"
                SELECT COUNT(*)
                FROM code_commits
                WHERE project_id = ?1
                  AND history_operation_id = 'history-op-completed'
                "#,
                params![project.project_id],
                |row| row.get(0),
            )
            .expect("duplicate commit count");
        assert_eq!(duplicate_count, 1);
    }

    #[test]
    fn maintenance_writes_local_diagnostics_report_for_support_triage() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_maintenance_report");
        let root = &project.repo_root;
        let (connection, _database_path) = open_code_rollback_database(root).expect("db");
        let request = SnapshotCaptureRequest {
            project_id: project.project_id.clone(),
            agent_session_id: project.agent_session_id.clone(),
            run_id: project.run_id.clone(),
            change_group_id: None,
            boundary_kind: CodeSnapshotBoundaryKind::Before,
            previous_snapshot_id: None,
            explicit_paths: BTreeSet::new(),
        };
        insert_pending_snapshot(&connection, root, &request, "code-snapshot-report-pending")
            .expect("pending snapshot");
        drop(connection);

        let report =
            maintain_code_rollback_storage(root, &project.project_id).expect("maintenance");

        assert!(report
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "code_snapshot_write_incomplete"));
        let report_path = code_rollback_storage_dir_for_repo(root)
            .join(DIAGNOSTICS_DIR)
            .join(MAINTENANCE_REPORT_FILE);
        let report_json = fs::read_to_string(report_path).expect("report json");
        let persisted: CodeRollbackMaintenanceReport =
            serde_json::from_str(&report_json).expect("decode report");
        assert_eq!(persisted.project_id, project.project_id);
        assert!(persisted
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "code_snapshot_write_incomplete"));
    }
}
