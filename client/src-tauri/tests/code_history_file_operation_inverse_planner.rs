use xero_desktop_lib::db::project_store::{
    plan_file_operation_inverse_patch, CodeExactFileState, CodeFileOperationCurrentState,
    CodeFileOperationInverseActionKind, CodeFileOperationInverseConflictKind,
    CodeFileOperationInversePatchPlanStatus, CodePatchFileKind, CodePatchFileOperation,
    CodePatchFileRecord, CodePatchMergePolicy,
};

#[test]
fn file_operation_inverse_planner_removes_unchanged_created_file() {
    let created = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let patch = patch_file(
        CodePatchFileOperation::Create,
        None,
        Some("src/new.bin"),
        None,
        Some(created.clone()),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(created),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(plan.status, CodeFileOperationInversePatchPlanStatus::Clean);
    assert_eq!(
        plan.actions[0].kind,
        CodeFileOperationInverseActionKind::RemovePath
    );
    assert_eq!(plan.actions[0].target_path, "src/new.bin");
}

#[test]
fn file_operation_inverse_planner_conflicts_when_created_file_changed() {
    let created = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let current = file_state(hash('b'), Some("blob-b"), Some(44), Some(0o644));
    let patch = patch_file(
        CodePatchFileOperation::Create,
        None,
        Some("src/new.bin"),
        None,
        Some(created),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(current),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(
        plan.conflicts[0].kind,
        CodeFileOperationInverseConflictKind::CurrentStateMismatch
    );
}

#[test]
fn file_operation_inverse_planner_restores_deleted_file_when_path_is_still_absent() {
    let deleted = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let patch = patch_file(
        CodePatchFileOperation::Delete,
        Some("src/old.bin"),
        None,
        Some(deleted.clone()),
        None,
    );

    let plan = plan_file_operation_inverse_patch(&patch, &CodeFileOperationCurrentState::default());

    assert_eq!(plan.status, CodeFileOperationInversePatchPlanStatus::Clean);
    assert_eq!(
        plan.actions[0].kind,
        CodeFileOperationInverseActionKind::RestorePath
    );
    assert_eq!(plan.actions[0].restore_state.as_ref(), Some(&deleted));
}

#[test]
fn file_operation_inverse_planner_conflicts_when_deleted_path_was_reused() {
    let deleted = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let reused = file_state(hash('b'), Some("blob-b"), Some(64), Some(0o644));
    let patch = patch_file(
        CodePatchFileOperation::Delete,
        Some("src/old.bin"),
        None,
        Some(deleted),
        None,
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_before: Some(reused),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(
        plan.conflicts[0].kind,
        CodeFileOperationInverseConflictKind::PathAlreadyExists
    );
}

#[test]
fn file_operation_inverse_planner_reverses_clean_rename() {
    let before = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let after = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let patch = patch_file(
        CodePatchFileOperation::Rename,
        Some("src/old.bin"),
        Some("src/new.bin"),
        Some(before.clone()),
        Some(after.clone()),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(after),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(plan.status, CodeFileOperationInversePatchPlanStatus::Clean);
    assert_eq!(
        plan.actions[0].kind,
        CodeFileOperationInverseActionKind::RenamePath
    );
    assert_eq!(plan.actions[0].source_path.as_deref(), Some("src/new.bin"));
    assert_eq!(plan.actions[0].target_path, "src/old.bin");
    assert_eq!(plan.actions[0].restore_state.as_ref(), Some(&before));
}

#[test]
fn file_operation_inverse_planner_conflicts_when_rename_source_path_exists() {
    let before = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let after = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let patch = patch_file(
        CodePatchFileOperation::Rename,
        Some("src/old.bin"),
        Some("src/new.bin"),
        Some(before.clone()),
        Some(after.clone()),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_before: Some(before),
            path_after: Some(after),
        },
    );

    assert_eq!(
        plan.conflicts[0].kind,
        CodeFileOperationInverseConflictKind::PathAlreadyExists
    );
}

#[test]
fn file_operation_inverse_planner_restores_mode_for_clean_mode_change() {
    let before = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let after = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o755));
    let patch = patch_file(
        CodePatchFileOperation::ModeChange,
        Some("script.sh"),
        Some("script.sh"),
        Some(before.clone()),
        Some(after.clone()),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(after),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(
        plan.actions[0].kind,
        CodeFileOperationInverseActionKind::RestoreMode
    );
    assert_eq!(plan.actions[0].restore_state.as_ref(), Some(&before));
}

#[test]
fn file_operation_inverse_planner_conflicts_when_mode_changed_again() {
    let before = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let after = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o755));
    let current = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o700));
    let patch = patch_file(
        CodePatchFileOperation::ModeChange,
        Some("script.sh"),
        Some("script.sh"),
        Some(before),
        Some(after),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(current),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(
        plan.conflicts[0].kind,
        CodeFileOperationInverseConflictKind::CurrentStateMismatch
    );
}

#[test]
fn file_operation_inverse_planner_restores_clean_symlink_change() {
    let before = symlink_state("target-v1", Some(0o777));
    let after = symlink_state("target-v2", Some(0o777));
    let patch = patch_file(
        CodePatchFileOperation::SymlinkChange,
        Some("latest"),
        Some("latest"),
        Some(before.clone()),
        Some(after.clone()),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(after),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(
        plan.actions[0].kind,
        CodeFileOperationInverseActionKind::RestoreSymlink
    );
    assert_eq!(plan.actions[0].restore_state.as_ref(), Some(&before));
}

#[test]
fn file_operation_inverse_planner_conflicts_when_symlink_target_changed_again() {
    let before = symlink_state("target-v1", Some(0o777));
    let after = symlink_state("target-v2", Some(0o777));
    let current = symlink_state("target-v3", Some(0o777));
    let patch = patch_file(
        CodePatchFileOperation::SymlinkChange,
        Some("latest"),
        Some("latest"),
        Some(before),
        Some(after),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(current),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(
        plan.conflicts[0].kind,
        CodeFileOperationInverseConflictKind::CurrentStateMismatch
    );
}

#[test]
fn file_operation_inverse_planner_restores_clean_binary_modify() {
    let before = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let after = file_state(hash('b'), Some("blob-b"), Some(64), Some(0o644));
    let patch = patch_file(
        CodePatchFileOperation::Modify,
        Some("image.bin"),
        Some("image.bin"),
        Some(before.clone()),
        Some(after.clone()),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(after),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(
        plan.actions[0].kind,
        CodeFileOperationInverseActionKind::RestorePath
    );
    assert_eq!(plan.actions[0].restore_state.as_ref(), Some(&before));
}

#[test]
fn file_operation_inverse_planner_conflicts_when_binary_modify_changed_again() {
    let before = file_state(hash('a'), Some("blob-a"), Some(42), Some(0o644));
    let after = file_state(hash('b'), Some("blob-b"), Some(64), Some(0o644));
    let current = file_state(hash('c'), Some("blob-c"), Some(71), Some(0o644));
    let patch = patch_file(
        CodePatchFileOperation::Modify,
        Some("image.bin"),
        Some("image.bin"),
        Some(before),
        Some(after),
    );

    let plan = plan_file_operation_inverse_patch(
        &patch,
        &CodeFileOperationCurrentState {
            path_after: Some(current),
            ..CodeFileOperationCurrentState::default()
        },
    );

    assert_eq!(
        plan.conflicts[0].kind,
        CodeFileOperationInverseConflictKind::CurrentStateMismatch
    );
}

fn patch_file(
    operation: CodePatchFileOperation,
    path_before: Option<&str>,
    path_after: Option<&str>,
    before: Option<CodeExactFileState>,
    after: Option<CodeExactFileState>,
) -> CodePatchFileRecord {
    CodePatchFileRecord {
        project_id: "project-1".into(),
        patchset_id: "patchset-1".into(),
        patch_file_id: "patch-file-1".into(),
        file_index: 0,
        path_before: path_before.map(Into::into),
        path_after: path_after.map(Into::into),
        operation,
        merge_policy: CodePatchMergePolicy::Exact,
        before_file_kind: before.as_ref().map(|state| state.kind),
        after_file_kind: after.as_ref().map(|state| state.kind),
        base_hash: before.as_ref().and_then(|state| state.content_hash.clone()),
        result_hash: after.as_ref().and_then(|state| state.content_hash.clone()),
        base_blob_id: before.as_ref().and_then(|state| state.blob_id.clone()),
        result_blob_id: after.as_ref().and_then(|state| state.blob_id.clone()),
        base_size: before.as_ref().and_then(|state| state.size),
        result_size: after.as_ref().and_then(|state| state.size),
        base_mode: before.as_ref().and_then(|state| state.mode),
        result_mode: after.as_ref().and_then(|state| state.mode),
        base_symlink_target: before
            .as_ref()
            .and_then(|state| state.symlink_target.clone()),
        result_symlink_target: after
            .as_ref()
            .and_then(|state| state.symlink_target.clone()),
        text_hunk_count: 0,
        created_at: "2026-05-06T12:00:00Z".into(),
        hunks: Vec::new(),
    }
}

fn file_state(
    content_hash: String,
    blob_id: Option<&str>,
    size: Option<u64>,
    mode: Option<u32>,
) -> CodeExactFileState {
    CodeExactFileState {
        kind: CodePatchFileKind::File,
        content_hash: Some(content_hash),
        blob_id: blob_id.map(Into::into),
        size,
        mode,
        symlink_target: None,
    }
}

fn symlink_state(target: &str, mode: Option<u32>) -> CodeExactFileState {
    CodeExactFileState {
        kind: CodePatchFileKind::Symlink,
        content_hash: None,
        blob_id: None,
        size: None,
        mode,
        symlink_target: Some(target.into()),
    }
}

fn hash(ch: char) -> String {
    std::iter::repeat(ch).take(64).collect()
}
