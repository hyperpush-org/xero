use sha2::{Digest, Sha256};
use xero_desktop_lib::db::project_store::{
    plan_text_file_inverse_patch, CodePatchFileKind, CodePatchFileOperation, CodePatchFileRecord,
    CodePatchHunkRecord, CodePatchMergePolicy, CodeTextInversePatchConflictKind,
    CodeTextInversePatchPlanStatus,
};

#[test]
fn text_inverse_planner_builds_clean_inverse_hunk() {
    let patch = text_modify_patch(
        "src/app.rs",
        "fn run() {\n    old_call();\n}\n",
        "fn run() {\n    new_call();\n}\n",
        CodePatchHunkRecord {
            removed_lines: vec!["    old_call();\n".into()],
            added_lines: vec!["    new_call();\n".into()],
            context_before: vec!["fn run() {\n".into()],
            context_after: vec!["}\n".into()],
            ..hunk("hunk-clean", 2, 1, 2, 1)
        },
    );

    let plan = plan_text_file_inverse_patch(&patch, Some("fn run() {\n    new_call();\n}\n"));

    assert!(plan.is_clean());
    assert_eq!(plan.status, CodeTextInversePatchPlanStatus::Clean);
    assert_eq!(
        plan.planned_content.as_deref(),
        Some("fn run() {\n    old_call();\n}\n")
    );
    assert_eq!(plan.inverse_hunks.len(), 1);
    assert_eq!(plan.inverse_hunks[0].source_hunk_id, "hunk-clean");
    assert_eq!(
        plan.inverse_hunks[0].inverse_removed_lines,
        vec!["    new_call();\n"]
    );
    assert_eq!(
        plan.inverse_hunks[0].inverse_added_lines,
        vec!["    old_call();\n"]
    );
}

#[test]
fn text_inverse_planner_preserves_unrelated_later_edits() {
    let patch = text_modify_patch(
        "src/app.rs",
        "fn run() {\n    old_call();\n}\n",
        "fn run() {\n    new_call();\n}\n",
        CodePatchHunkRecord {
            removed_lines: vec!["    old_call();\n".into()],
            added_lines: vec!["    new_call();\n".into()],
            context_before: vec!["fn run() {\n".into()],
            context_after: vec!["}\n".into()],
            ..hunk("hunk-preserve", 2, 1, 2, 1)
        },
    );

    let plan = plan_text_file_inverse_patch(
        &patch,
        Some("use crate::later;\nfn run() {\n    new_call();\n}\n// later edit\n"),
    );

    assert_eq!(
        plan.planned_content.as_deref(),
        Some("use crate::later;\nfn run() {\n    old_call();\n}\n// later edit\n")
    );
}

#[test]
fn text_inverse_planner_reinserts_deleted_text_with_context() {
    let patch = text_modify_patch(
        "src/app.rs",
        "fn run() {\n    old_call();\n}\n",
        "fn run() {\n}\n",
        CodePatchHunkRecord {
            removed_lines: vec!["    old_call();\n".into()],
            added_lines: Vec::new(),
            context_before: vec!["fn run() {\n".into()],
            context_after: vec!["}\n".into()],
            ..hunk("hunk-reinsert", 2, 1, 2, 0)
        },
    );

    let plan = plan_text_file_inverse_patch(
        &patch,
        Some("use crate::later;\nfn run() {\n}\n// later edit\n"),
    );

    assert_eq!(
        plan.planned_content.as_deref(),
        Some("use crate::later;\nfn run() {\n    old_call();\n}\n// later edit\n")
    );
    assert_eq!(plan.inverse_hunks[0].current_line_count, 0);
}

#[test]
fn text_inverse_planner_conflicts_when_selected_lines_changed() {
    let patch = text_modify_patch(
        "src/app.rs",
        "fn run() {\n    old_call();\n}\n",
        "fn run() {\n    new_call();\n}\n",
        CodePatchHunkRecord {
            removed_lines: vec!["    old_call();\n".into()],
            added_lines: vec!["    new_call();\n".into()],
            context_before: vec!["fn run() {\n".into()],
            context_after: vec!["}\n".into()],
            ..hunk("hunk-overlap", 2, 1, 2, 1)
        },
    );

    let plan = plan_text_file_inverse_patch(&patch, Some("fn run() {\n    newer_call();\n}\n"));

    assert_eq!(plan.status, CodeTextInversePatchPlanStatus::Conflicted);
    assert_eq!(plan.planned_content, None);
    assert_eq!(plan.conflicts.len(), 1);
    assert_eq!(
        plan.conflicts[0].kind,
        CodeTextInversePatchConflictKind::TextOverlap
    );
    assert_eq!(plan.conflicts[0].hunk_ids, vec!["hunk-overlap"]);
}

#[test]
fn text_inverse_planner_conflicts_when_selected_lines_are_ambiguous() {
    let patch = text_modify_patch(
        "src/app.rs",
        "old_call();\nnew_call();\n",
        "new_call();\nnew_call();\n",
        CodePatchHunkRecord {
            removed_lines: vec!["old_call();\n".into()],
            added_lines: vec!["new_call();\n".into()],
            context_before: Vec::new(),
            context_after: Vec::new(),
            ..hunk("hunk-ambiguous", 1, 1, 1, 1)
        },
    );

    let plan = plan_text_file_inverse_patch(&patch, Some("new_call();\nnew_call();\n"));

    assert_eq!(plan.status, CodeTextInversePatchPlanStatus::Conflicted);
    assert_eq!(
        plan.conflicts[0].kind,
        CodeTextInversePatchConflictKind::TextOverlap
    );
}

#[test]
fn text_inverse_planner_conflicts_when_current_file_is_missing() {
    let patch = text_modify_patch(
        "src/app.rs",
        "fn run() {\n    old_call();\n}\n",
        "fn run() {\n    new_call();\n}\n",
        CodePatchHunkRecord {
            removed_lines: vec!["    old_call();\n".into()],
            added_lines: vec!["    new_call();\n".into()],
            context_before: vec!["fn run() {\n".into()],
            context_after: vec!["}\n".into()],
            ..hunk("hunk-missing", 2, 1, 2, 1)
        },
    );

    let plan = plan_text_file_inverse_patch(&patch, None);

    assert_eq!(plan.status, CodeTextInversePatchPlanStatus::Conflicted);
    assert_eq!(
        plan.conflicts[0].kind,
        CodeTextInversePatchConflictKind::FileMissing
    );
    assert_eq!(plan.conflicts[0].path, "src/app.rs");
}

#[test]
fn text_inverse_planner_handles_final_newline_edge_cases() {
    let patch = text_modify_patch(
        "notes.txt",
        "alpha\nbeta",
        "alpha\nbeta\n",
        CodePatchHunkRecord {
            removed_lines: vec!["beta".into()],
            added_lines: vec!["beta\n".into()],
            context_before: vec!["alpha\n".into()],
            context_after: Vec::new(),
            ..hunk("hunk-newline", 2, 1, 2, 1)
        },
    );

    let clean = plan_text_file_inverse_patch(&patch, Some("alpha\nbeta\n"));
    assert_eq!(clean.planned_content.as_deref(), Some("alpha\nbeta"));

    let conflicted = plan_text_file_inverse_patch(&patch, Some("alpha\nbeta\ngamma\n"));
    assert_eq!(
        conflicted.status,
        CodeTextInversePatchPlanStatus::Conflicted
    );
    assert_eq!(
        conflicted.conflicts[0].kind,
        CodeTextInversePatchConflictKind::TextOverlap
    );
}

fn text_modify_patch(
    path: &str,
    base: &str,
    selected: &str,
    hunk: CodePatchHunkRecord,
) -> CodePatchFileRecord {
    CodePatchFileRecord {
        project_id: "project-1".into(),
        patchset_id: "patchset-1".into(),
        patch_file_id: "patch-file-1".into(),
        file_index: 0,
        path_before: Some(path.into()),
        path_after: Some(path.into()),
        operation: CodePatchFileOperation::Modify,
        merge_policy: CodePatchMergePolicy::Text,
        before_file_kind: Some(CodePatchFileKind::File),
        after_file_kind: Some(CodePatchFileKind::File),
        base_hash: Some(sha256_hex(base.as_bytes())),
        result_hash: Some(sha256_hex(selected.as_bytes())),
        base_blob_id: None,
        result_blob_id: None,
        base_size: Some(base.len() as u64),
        result_size: Some(selected.len() as u64),
        base_mode: Some(0o644),
        result_mode: Some(0o644),
        base_symlink_target: None,
        result_symlink_target: None,
        text_hunk_count: 1,
        created_at: "2026-05-06T12:00:00Z".into(),
        hunks: vec![hunk],
    }
}

fn hunk(
    hunk_id: &str,
    base_start_line: u32,
    base_line_count: u32,
    result_start_line: u32,
    result_line_count: u32,
) -> CodePatchHunkRecord {
    CodePatchHunkRecord {
        project_id: "project-1".into(),
        patch_file_id: "patch-file-1".into(),
        hunk_id: hunk_id.into(),
        hunk_index: 0,
        base_start_line,
        base_line_count,
        result_start_line,
        result_line_count,
        removed_lines: Vec::new(),
        added_lines: Vec::new(),
        context_before: Vec::new(),
        context_after: Vec::new(),
        created_at: "2026-05-06T12:00:00Z".into(),
    }
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
