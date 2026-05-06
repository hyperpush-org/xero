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
use time::{format_description::well_known::Rfc3339, OffsetDateTime};

use crate::{
    auth::now_timestamp,
    commands::{CommandError, CommandResult},
    db::{database_path_for_repo, project_app_data_dir_for_repo},
};

use super::{
    code_history::{
        advance_code_workspace_epoch, ensure_code_workspace_head, persist_code_patchset_commit,
        AdvanceCodeWorkspaceEpochRequest, CodeHistoryCommitKind, CodePatchFileInput,
        CodePatchFileKind, CodePatchFileOperation, CodePatchHunkInput, CodePatchMergePolicy,
        CodePatchsetCommitInput,
    },
    open_runtime_database, read_project_row,
};

const SNAPSHOT_SCHEMA: &str = "xero.code_snapshot.v1";
const CODE_ROLLBACK_DIR: &str = "code-rollback";
const BLOB_DIR: &str = "blobs";
const DIAGNOSTICS_DIR: &str = "diagnostics";
const MAINTENANCE_REPORT_FILE: &str = "latest-maintenance.json";
const DEFAULT_UNREFERENCED_BLOB_RETENTION_SECONDS: i64 = 7 * 24 * 60 * 60;

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
pub struct CodeSnapshotRestoreOutcome {
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
    validate_change_group_input(&input)?;
    let change_group_id = input
        .change_group_id
        .clone()
        .unwrap_or_else(|| generate_id("code-change"));
    let explicit_paths = broad_capture_explicit_paths(repo_root, &input.project_id)?;
    insert_change_group_open(repo_root, &input, &change_group_id)?;
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
        targets: Vec::new(),
    })
}

pub fn complete_broad_capture(
    repo_root: &Path,
    handle: CodeRollbackCaptureHandle,
) -> CommandResult<CompletedCodeChangeGroup> {
    let explicit_paths = broad_capture_explicit_paths(repo_root, &handle.project_id)?;
    let after_snapshot = match capture_code_snapshot_internal(
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

pub fn restore_code_snapshot(
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

    let restore_lock = project_restore_lock(repo_root, project_id)?;
    let _restore_guard = restore_lock.lock().map_err(|_| {
        CommandError::system_fault(
            "code_rollback_lock_poisoned",
            "Xero could not enter the code rollback restore lane.",
        )
    })?;

    let target = load_rollback_target_change_group(repo_root, project_id, target_change_group_id)?;
    let operation_id = generate_id("code-rollback");
    let result_change_group_id = generate_id("code-change");
    let result_summary = format!("Rollback {}", target.summary_label);
    let result_group_input = CodeChangeGroupInput {
        project_id: target.project_id.clone(),
        agent_session_id: target.agent_session_id.clone(),
        run_id: target.run_id.clone(),
        change_group_id: Some(result_change_group_id.clone()),
        parent_change_group_id: Some(target.change_group_id.clone()),
        tool_call_id: None,
        runtime_event_id: None,
        conversation_sequence: None,
        change_kind: CodeChangeKind::Rollback,
        summary_label: result_summary,
        restore_state: CodeChangeRestoreState::SnapshotAvailable,
    };
    let explicit_paths = broad_capture_explicit_paths(repo_root, &target.project_id)?;
    insert_change_group_open(repo_root, &result_group_input, &result_change_group_id)?;
    if let Err(error) = insert_pending_rollback_operation(
        repo_root,
        &operation_id,
        &target,
        &result_change_group_id,
    ) {
        let _ = mark_change_group_failed(
            repo_root,
            &target.project_id,
            &result_change_group_id,
            &error,
        );
        return Err(error);
    }

    let pre_rollback_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: target.project_id.clone(),
            agent_session_id: target.agent_session_id.clone(),
            run_id: target.run_id.clone(),
            change_group_id: Some(result_change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::PreRollback,
            previous_snapshot_id: None,
            explicit_paths: explicit_paths.clone(),
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            mark_failed_rollback_attempt(
                repo_root,
                &target.project_id,
                &operation_id,
                &result_change_group_id,
                &error,
            );
            return Err(error);
        }
    };

    if let Err(error) = update_change_group_before_snapshot(
        repo_root,
        &target.project_id,
        &result_change_group_id,
        &pre_rollback_snapshot.snapshot_id,
    )
    .and_then(|()| {
        update_rollback_operation_pre_snapshot(
            repo_root,
            &target.project_id,
            &operation_id,
            &pre_rollback_snapshot.snapshot_id,
        )
    }) {
        mark_failed_rollback_attempt(
            repo_root,
            &target.project_id,
            &operation_id,
            &result_change_group_id,
            &error,
        );
        return Err(error);
    }

    let restore_outcome = match restore_target_snapshot(repo_root, &target) {
        Ok(outcome) => outcome,
        Err(error) => {
            mark_failed_rollback_attempt(
                repo_root,
                &target.project_id,
                &operation_id,
                &result_change_group_id,
                &error,
            );
            return Err(error);
        }
    };

    let post_rollback_snapshot = match capture_code_snapshot_internal(
        repo_root,
        SnapshotCaptureRequest {
            project_id: target.project_id.clone(),
            agent_session_id: target.agent_session_id.clone(),
            run_id: target.run_id.clone(),
            change_group_id: Some(result_change_group_id.clone()),
            boundary_kind: CodeSnapshotBoundaryKind::PostRollback,
            previous_snapshot_id: Some(pre_rollback_snapshot.snapshot_id.clone()),
            explicit_paths: explicit_paths.clone(),
        },
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => {
            mark_failed_rollback_attempt(
                repo_root,
                &target.project_id,
                &operation_id,
                &result_change_group_id,
                &error,
            );
            return Err(error);
        }
    };

    let completed_group = match persist_broad_file_versions(
        repo_root,
        &CodeRollbackCaptureHandle {
            project_id: target.project_id.clone(),
            agent_session_id: target.agent_session_id.clone(),
            run_id: target.run_id.clone(),
            change_group_id: result_change_group_id.clone(),
            before_snapshot_id: pre_rollback_snapshot.snapshot_id.clone(),
            targets: Vec::new(),
        },
        &post_rollback_snapshot.snapshot_id,
        &explicit_paths,
    ) {
        Ok(group) => group,
        Err(error) => {
            mark_failed_rollback_attempt(
                repo_root,
                &target.project_id,
                &operation_id,
                &result_change_group_id,
                &error,
            );
            return Err(error);
        }
    };

    if let Err(error) = complete_rollback_operation(
        repo_root,
        &target.project_id,
        &operation_id,
        &completed_group.affected_files,
    ) {
        mark_failed_rollback_attempt(
            repo_root,
            &target.project_id,
            &operation_id,
            &result_change_group_id,
            &error,
        );
        return Err(error);
    }

    Ok(AppliedCodeRollback {
        project_id: target.project_id,
        agent_session_id: target.agent_session_id,
        run_id: target.run_id,
        operation_id,
        target_change_group_id: target.change_group_id,
        target_snapshot_id: target.before_snapshot_id,
        pre_rollback_snapshot_id: pre_rollback_snapshot.snapshot_id,
        result_change_group_id,
        restored_paths: restore_outcome.restored_paths,
        removed_paths: restore_outcome.removed_paths,
        affected_files: completed_group.affected_files,
    })
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

    let pending_operations = collect_pending_rollback_operations(&connection, project_id)?;
    for (operation_id, result_change_group_id) in pending_operations {
        let failure = CommandError::retryable(
            "code_rollback_operation_incomplete",
            format!(
                "Rollback operation `{operation_id}` was still pending at startup and needs inspection."
            ),
        );
        mark_rollback_operation_failed(&connection, project_id, &operation_id, &failure)?;
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

    let completed = CompletedCodeChangeGroup {
        project_id: handle.project_id.clone(),
        agent_session_id: handle.agent_session_id.clone(),
        run_id: handle.run_id.clone(),
        change_group_id: handle.change_group_id.clone(),
        before_snapshot_id: handle.before_snapshot_id.clone(),
        after_snapshot_id: after_snapshot_id.into(),
        file_version_count: affected_files.len(),
        affected_files,
    };
    persist_exact_capture_history_commit(repo_root, handle, &before_manifest, &after_manifest)?;

    Ok(completed)
}

fn persist_broad_file_versions(
    repo_root: &Path,
    handle: &CodeRollbackCaptureHandle,
    after_snapshot_id: &str,
    explicit_paths: &BTreeSet<String>,
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
    let all_paths = before_entries
        .keys()
        .chain(after_entries.keys())
        .cloned()
        .collect::<BTreeSet<_>>();

    let tx = connection.unchecked_transaction().map_err(|error| {
        CommandError::system_fault(
            "code_change_group_transaction_failed",
            format!("Xero could not start code change group transaction: {error}"),
        )
    })?;

    let mut affected_files = Vec::new();
    for path in all_paths {
        let before_entry = before_entries.get(&path);
        let after_entry = after_entries.get(&path);
        if before_entry == after_entry {
            continue;
        }
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

    let completed = CompletedCodeChangeGroup {
        project_id: handle.project_id.clone(),
        agent_session_id: handle.agent_session_id.clone(),
        run_id: handle.run_id.clone(),
        change_group_id: handle.change_group_id.clone(),
        before_snapshot_id: handle.before_snapshot_id.clone(),
        after_snapshot_id: after_snapshot_id.into(),
        file_version_count: affected_files.len(),
        affected_files,
    };
    persist_broad_capture_history_commit(repo_root, handle, &before_manifest, &after_manifest)?;

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
) -> CommandResult<()> {
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
        return Ok(());
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

    persist_code_patchset_commit(
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

    Ok(())
}

fn persist_broad_capture_history_commit(
    repo_root: &Path,
    handle: &CodeRollbackCaptureHandle,
    before_manifest: &CodeSnapshotManifest,
    after_manifest: &CodeSnapshotManifest,
) -> CommandResult<()> {
    let files = patch_files_from_manifest_diff(
        repo_root,
        &handle.project_id,
        before_manifest,
        after_manifest,
    )?;
    if files.is_empty() {
        return Ok(());
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

    persist_code_patchset_commit(
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

    Ok(())
}

fn patch_files_from_manifest_diff(
    repo_root: &Path,
    project_id: &str,
    before_manifest: &CodeSnapshotManifest,
    after_manifest: &CodeSnapshotManifest,
) -> CommandResult<Vec<CodePatchFileInput>> {
    let before_entries = before_manifest.entry_map();
    let after_entries = after_manifest.entry_map();
    let all_paths = before_entries
        .keys()
        .chain(after_entries.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut files = Vec::new();

    for path in all_paths {
        let before_entry = before_entries.get(&path);
        let after_entry = after_entries.get(&path);
        if before_entry == after_entry {
            continue;
        }
        let target = CodeRollbackCaptureTarget {
            path_before: before_entry.map(|entry| entry.path.clone()),
            path_after: after_entry.map(|entry| entry.path.clone()),
            operation: None,
            explicitly_edited: false,
        };
        let operation = infer_file_operation(&target, before_entry, after_entry);
        let Some(file) = patch_file_input_from_entries(
            repo_root,
            project_id,
            files.len(),
            operation,
            before_entry,
            after_entry,
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

    Ok((
        CodePatchMergePolicy::Text,
        build_single_text_hunk(patch_file_id, &before_text, &after_text),
    ))
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

fn insert_pending_rollback_operation(
    repo_root: &Path,
    operation_id: &str,
    target: &RollbackTargetChangeGroup,
    result_change_group_id: &str,
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

fn update_rollback_operation_pre_snapshot(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    pre_rollback_snapshot_id: &str,
) -> CommandResult<()> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    connection
        .execute(
            r#"
            UPDATE code_rollback_operations
            SET pre_rollback_snapshot_id = ?3
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![project_id, operation_id, pre_rollback_snapshot_id],
        )
        .map_err(|error| {
            CommandError::system_fault(
                "code_rollback_operation_update_failed",
                format!(
                    "Xero could not attach pre-rollback snapshot `{pre_rollback_snapshot_id}` to operation `{operation_id}` in {}: {error}",
                    database_path.display()
                ),
            )
        })?;
    Ok(())
}

fn restore_target_snapshot(
    repo_root: &Path,
    target: &RollbackTargetChangeGroup,
) -> CommandResult<CodeSnapshotRestoreOutcome> {
    let (connection, database_path) = open_code_rollback_database(repo_root)?;
    let manifest = load_completed_snapshot_manifest(
        &connection,
        &database_path,
        &target.project_id,
        &target.before_snapshot_id,
    )?;
    let blob_bytes =
        load_required_blob_bytes(&connection, repo_root, &target.project_id, &manifest)?;
    drop(connection);

    apply_manifest_to_project(
        repo_root,
        &target.before_snapshot_id,
        &manifest,
        &blob_bytes,
    )
}

fn complete_rollback_operation(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
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
                affected_files_json = ?3,
                completed_at = ?4
            WHERE project_id = ?1
              AND operation_id = ?2
            "#,
            params![project_id, operation_id, affected_files_json, completed_at],
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

fn mark_failed_rollback_attempt(
    repo_root: &Path,
    project_id: &str,
    operation_id: &str,
    result_change_group_id: &str,
    error: &CommandError,
) {
    if let Ok((connection, _database_path)) = open_code_rollback_database(repo_root) {
        let _ = mark_rollback_operation_failed(&connection, project_id, operation_id, error);
    }
    let _ = mark_change_group_failed(repo_root, project_id, result_change_group_id, error);
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
) -> CommandResult<CodeSnapshotManifest> {
    let previous_entries = previous_manifest.map(CodeSnapshotManifest::entry_map);
    let mut entries = BTreeMap::new();

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
        let relative_path = repo_relative_path(repo_root, entry.path())?;
        if let Some(snapshot_entry) = capture_manifest_entry(
            repo_root,
            connection,
            project_id,
            &relative_path,
            previous_entries.as_ref(),
            true,
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
        )?;
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
    Ok(reachable)
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
            CodeChangeGroupInput {
                project_id: self.project_id.clone(),
                agent_session_id: self.agent_session_id.clone(),
                run_id: self.run_id.clone(),
                change_group_id: None,
                parent_change_group_id: None,
                tool_call_id: Some(format!("tool-{label}")),
                runtime_event_id: None,
                conversation_sequence: None,
                change_kind: CodeChangeKind::FileTool,
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
    fn apply_code_rollback_restores_selected_boundary_and_can_be_rolled_back() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_apply_three_edits");
        let root = &project.repo_root;
        fs::write(root.join("tracked.txt"), "A\n").expect("baseline");

        let group_1 = capture_modify(&project, "edit 1", "tracked.txt", "B\n");
        let group_2 = capture_modify(&project, "edit 2", "tracked.txt", "C\n");
        let group_3 = capture_modify(&project, "edit 3", "tracked.txt", "D\n");
        fs::write(root.join("tracked.txt"), "human edit after edit 3\n").expect("human edit");

        let rollback = apply_code_rollback(root, &project.project_id, &group_2.change_group_id)
            .expect("apply rollback");

        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read rolled back file"),
            "B\n"
        );
        assert_eq!(rollback.target_snapshot_id, group_2.before_snapshot_id);
        assert_eq!(rollback.affected_files.len(), 1);
        assert_eq!(
            rollback.affected_files[0].path_before.as_deref(),
            Some("tracked.txt")
        );

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
            Some("edit 2")
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
        assert_eq!(parent_change_group_id, group_2.change_group_id);
        assert_eq!(before_snapshot_id, rollback.pre_rollback_snapshot_id);

        let third_group_status: String = connection
            .query_row(
                "SELECT status FROM code_change_groups WHERE project_id = ?1 AND change_group_id = ?2",
                params![project.project_id, group_3.change_group_id],
                |row| row.get(0),
            )
            .expect("third group still recorded");
        assert_eq!(third_group_status, "completed");
        assert_ne!(group_1.change_group_id, rollback.result_change_group_id);

        let undo = apply_code_rollback(root, &project.project_id, &rollback.result_change_group_id)
            .expect("rollback the rollback");
        assert_eq!(
            fs::read_to_string(root.join("tracked.txt")).expect("read undo file"),
            "human edit after edit 3\n"
        );
        assert_eq!(undo.target_snapshot_id, rollback.pre_rollback_snapshot_id);
    }

    #[test]
    fn retention_prunes_only_unreachable_blobs_and_preserves_rollback_snapshots() {
        let _guard = PROJECT_DB_LOCK.lock().expect("project db lock");
        let project = TestProject::new("rollback_retention");
        let root = &project.repo_root;
        fs::write(root.join("tracked.txt"), "A\n").expect("baseline");
        let _group_1 = capture_modify(&project, "edit 1", "tracked.txt", "B\n");
        let group_2 = capture_modify(&project, "edit 2", "tracked.txt", "C\n");
        fs::write(root.join("tracked.txt"), "human edit\n").expect("human edit");
        let rollback = apply_code_rollback(root, &project.project_id, &group_2.change_group_id)
            .expect("apply rollback");
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
            fs::read_to_string(root.join("tracked.txt")).expect("read restored human edit"),
            "human edit\n"
        );
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
        let completed = capture_modify(&project, "missing target blob", "file.txt", "after\n");
        fs::write(root.join("file.txt"), "later\n").expect("later");

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
        assert_eq!(
            fs::read_to_string(root.join("file.txt")).expect("read unchanged file"),
            "later\n"
        );

        let connection = Connection::open(db::database_path_for_repo(root)).expect("open db");
        let (status, failure_code, pre_snapshot_id, result_change_group_id): (
            String,
            String,
            Option<String>,
            String,
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
        assert!(pre_snapshot_id.is_some());

        let result_status: String = connection
            .query_row(
                "SELECT status FROM code_change_groups WHERE project_id = ?1 AND change_group_id = ?2",
                params![project.project_id, result_change_group_id],
                |row| row.get(0),
            )
            .expect("failed result change group");
        assert_eq!(result_status, "failed");
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
            result_change_group_id,
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
