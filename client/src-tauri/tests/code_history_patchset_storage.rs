use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard},
};

use rusqlite::{params, Connection};
use tempfile::{tempdir, TempDir};
use xero_desktop_lib::{
    auth::now_timestamp,
    commands::RuntimeAgentIdDto,
    db::{
        self, database_path_for_repo,
        project_store::{
            self, CodeHistoryCommitKind, CodePatchFileInput, CodePatchFileKind,
            CodePatchFileOperation, CodePatchHunkInput, CodePatchMergePolicy,
            CodePatchsetCommitInput, NewAgentRunRecord, DEFAULT_AGENT_SESSION_ID,
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
    _guard: MutexGuard<'static, ()>,
}

#[test]
fn code_patchset_commit_storage_round_trips_modify_create_delete_patch_metadata() {
    let project = seed_project("project-history-patchset-storage");
    let change_group_id = seed_completed_change_group(&project);
    let base_modify_blob = blob_id('a');
    let result_modify_blob = blob_id('b');
    let result_create_blob = blob_id('c');
    let base_delete_blob = blob_id('d');
    for (blob_id, size_bytes) in [
        (&base_modify_blob, 16_i64),
        (&result_modify_blob, 28_i64),
        (&result_create_blob, 11_i64),
        (&base_delete_blob, 13_i64),
    ] {
        insert_blob_row(&project, blob_id, size_bytes);
    }

    let input = CodePatchsetCommitInput {
        project_id: project.project_id.clone(),
        commit_id: "code-commit-synthetic-1".into(),
        parent_commit_id: None,
        tree_id: "code-tree-after-1".into(),
        parent_tree_id: Some("code-tree-before-1".into()),
        patchset_id: "code-patchset-synthetic-1".into(),
        change_group_id: change_group_id.clone(),
        history_operation_id: None,
        agent_session_id: project.agent_session_id.clone(),
        run_id: project.run_id.clone(),
        tool_call_id: Some("tool-call-1".into()),
        runtime_event_id: Some(7),
        conversation_sequence: Some(3),
        commit_kind: CodeHistoryCommitKind::ChangeGroup,
        summary_label: "synthetic modify create delete".into(),
        workspace_epoch: 5,
        created_at: "2026-05-06T12:00:00Z".into(),
        completed_at: "2026-05-06T12:00:01Z".into(),
        files: vec![
            CodePatchFileInput {
                patch_file_id: "patch-file-modify".into(),
                path_before: Some("src/app.rs".into()),
                path_after: Some("src/app.rs".into()),
                operation: CodePatchFileOperation::Modify,
                merge_policy: CodePatchMergePolicy::Text,
                before_file_kind: Some(CodePatchFileKind::File),
                after_file_kind: Some(CodePatchFileKind::File),
                base_hash: Some(base_modify_blob.clone()),
                result_hash: Some(result_modify_blob.clone()),
                base_blob_id: Some(base_modify_blob.clone()),
                result_blob_id: Some(result_modify_blob.clone()),
                base_size: Some(16),
                result_size: Some(28),
                base_mode: Some(0o644),
                result_mode: Some(0o644),
                base_symlink_target: None,
                result_symlink_target: None,
                hunks: vec![CodePatchHunkInput {
                    hunk_id: "hunk-modify-1".into(),
                    hunk_index: 0,
                    base_start_line: 2,
                    base_line_count: 1,
                    result_start_line: 2,
                    result_line_count: 2,
                    removed_lines: vec!["old_call();".into()],
                    added_lines: vec!["new_call();".into(), "audit_call();".into()],
                    context_before: vec!["fn run() {".into()],
                    context_after: vec!["}".into()],
                }],
            },
            CodePatchFileInput {
                patch_file_id: "patch-file-create".into(),
                path_before: None,
                path_after: Some("src/new.rs".into()),
                operation: CodePatchFileOperation::Create,
                merge_policy: CodePatchMergePolicy::Exact,
                before_file_kind: None,
                after_file_kind: Some(CodePatchFileKind::File),
                base_hash: None,
                result_hash: Some(result_create_blob.clone()),
                base_blob_id: None,
                result_blob_id: Some(result_create_blob.clone()),
                base_size: None,
                result_size: Some(11),
                base_mode: None,
                result_mode: Some(0o644),
                base_symlink_target: None,
                result_symlink_target: None,
                hunks: Vec::new(),
            },
            CodePatchFileInput {
                patch_file_id: "patch-file-delete".into(),
                path_before: Some("src/old.rs".into()),
                path_after: None,
                operation: CodePatchFileOperation::Delete,
                merge_policy: CodePatchMergePolicy::Exact,
                before_file_kind: Some(CodePatchFileKind::File),
                after_file_kind: None,
                base_hash: Some(base_delete_blob.clone()),
                result_hash: None,
                base_blob_id: Some(base_delete_blob.clone()),
                result_blob_id: None,
                base_size: Some(13),
                result_size: None,
                base_mode: Some(0o644),
                result_mode: None,
                base_symlink_target: None,
                result_symlink_target: None,
                hunks: Vec::new(),
            },
        ],
    };

    let persisted = project_store::persist_code_patchset_commit(&project.repo_root, &input)
        .expect("persist patchset commit");
    let reloaded = project_store::read_code_patchset_commit(
        &project.repo_root,
        &project.project_id,
        &input.commit_id,
    )
    .expect("read patchset commit")
    .expect("patchset commit exists");

    assert_eq!(reloaded, persisted);
    assert_eq!(reloaded.commit.agent_session_id, project.agent_session_id);
    assert_eq!(reloaded.commit.run_id, project.run_id);
    assert_eq!(reloaded.commit.tool_call_id.as_deref(), Some("tool-call-1"));
    assert_eq!(reloaded.commit.runtime_event_id, Some(7));
    assert_eq!(reloaded.commit.conversation_sequence, Some(3));
    assert_eq!(reloaded.patchset.file_count, 3);
    assert_eq!(reloaded.patchset.text_hunk_count, 1);
    assert_eq!(reloaded.files.len(), 3);

    let modify = &reloaded.files[0];
    assert_eq!(modify.operation, CodePatchFileOperation::Modify);
    assert_eq!(
        modify.base_blob_id.as_deref(),
        Some(base_modify_blob.as_str())
    );
    assert_eq!(
        modify.result_blob_id.as_deref(),
        Some(result_modify_blob.as_str())
    );
    assert_eq!(modify.base_hash.as_deref(), Some(base_modify_blob.as_str()));
    assert_eq!(modify.hunks.len(), 1);
    assert_eq!(modify.hunks[0].removed_lines, vec!["old_call();"]);
    assert_eq!(
        modify.hunks[0].added_lines,
        vec!["new_call();", "audit_call();"]
    );
    assert_eq!(modify.hunks[0].context_before, vec!["fn run() {"]);
    assert_eq!(modify.hunks[0].context_after, vec!["}"]);

    let created = &reloaded.files[1];
    assert_eq!(created.operation, CodePatchFileOperation::Create);
    assert_eq!(
        created.result_blob_id.as_deref(),
        Some(result_create_blob.as_str())
    );
    assert_eq!(created.base_blob_id, None);

    let deleted = &reloaded.files[2];
    assert_eq!(deleted.operation, CodePatchFileOperation::Delete);
    assert_eq!(
        deleted.base_blob_id.as_deref(),
        Some(base_delete_blob.as_str())
    );
    assert_eq!(deleted.result_blob_id, None);
}

fn seed_project(project_id: &str) -> TestProject {
    let guard = PROJECT_DB_LOCK.lock().expect("project db lock");
    let tempdir = tempdir().expect("tempdir");
    let app_data_dir = tempdir.path().join("app-data");
    let repo_root = tempdir.path().join("repo");
    fs::create_dir_all(repo_root.join("src")).expect("repo root");
    db::configure_project_database_paths(&app_data_dir.join("global.db"));
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let repository = canonical_repository(&canonical_root, project_id);
    db::import_project_with_origin(
        &repository,
        ProjectOrigin::Brownfield,
        &ImportFailpoints::default(),
    )
    .expect("import project");

    let run_id = "run-history-1".to_string();
    project_store::insert_agent_run(
        &canonical_root,
        &NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: Some("engineer".into()),
            agent_definition_version: Some(1),
            project_id: project_id.into(),
            agent_session_id: DEFAULT_AGENT_SESSION_ID.into(),
            run_id: run_id.clone(),
            provider_id: "test-provider".into(),
            model_id: "test-model".into(),
            prompt: "test prompt".into(),
            system_prompt: "test system prompt".into(),
            now: now_timestamp(),
        },
    )
    .expect("insert agent run");

    TestProject {
        _tempdir: tempdir,
        repo_root: canonical_root,
        project_id: project_id.into(),
        agent_session_id: DEFAULT_AGENT_SESSION_ID.into(),
        run_id,
        _guard: guard,
    }
}

fn seed_completed_change_group(project: &TestProject) -> String {
    let change_group_id = "code-change-synthetic-1".to_string();
    let connection = Connection::open(database_path_for_repo(&project.repo_root)).expect("open db");
    connection
        .execute(
            r#"
            INSERT INTO code_change_groups (
                project_id,
                agent_session_id,
                run_id,
                change_group_id,
                tool_call_id,
                runtime_event_id,
                conversation_sequence,
                change_kind,
                summary_label,
                restore_state,
                status,
                started_at,
                completed_at
            )
            VALUES (?1, ?2, ?3, ?4, 'tool-call-1', 7, 3, 'file_tool', 'synthetic modify create delete', 'snapshot_available', 'completed', ?5, ?6)
            "#,
            params![
                project.project_id,
                project.agent_session_id,
                project.run_id,
                change_group_id,
                "2026-05-06T12:00:00Z",
                "2026-05-06T12:00:01Z",
            ],
        )
        .expect("insert completed change group");
    change_group_id
}

fn insert_blob_row(project: &TestProject, blob_id: &str, size_bytes: i64) {
    let connection = Connection::open(database_path_for_repo(&project.repo_root)).expect("open db");
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
                size_bytes,
                format!("code-rollback/blobs/{blob_id}"),
                "2026-05-06T12:00:00Z",
            ],
        )
        .expect("insert blob row");
}

fn canonical_repository(root_path: &Path, project_id: &str) -> CanonicalRepository {
    CanonicalRepository {
        project_id: project_id.into(),
        repository_id: format!("repo-{project_id}"),
        root_path: root_path.to_path_buf(),
        root_path_string: root_path.to_string_lossy().into_owned(),
        common_git_dir: root_path.join(".git"),
        display_name: "repo".into(),
        branch_name: Some("main".into()),
        head_sha: Some("abc123".into()),
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

fn blob_id(fill: char) -> String {
    std::iter::repeat(fill).take(64).collect()
}
