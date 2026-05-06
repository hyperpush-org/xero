use std::{
    fs,
    path::PathBuf,
    sync::{Mutex, MutexGuard},
};

use rusqlite::{params, Connection};
use tempfile::{tempdir, TempDir};
use xero_desktop_lib::{
    db::{
        self, database_path_for_repo,
        project_store::{self, AdvanceCodeWorkspaceEpochRequest},
    },
    git::repository::CanonicalRepository,
    state::DesktopState,
};

static PROJECT_DB_LOCK: Mutex<()> = Mutex::new(());

struct TestProject {
    _tempdir: TempDir,
    repo_root: PathBuf,
    project_id: String,
    _guard: MutexGuard<'static, ()>,
}

#[test]
fn code_workspace_head_storage_initializes_advances_path_epochs_and_reopens() {
    let project = seed_project("project-history-storage");

    let initial_head =
        project_store::ensure_code_workspace_head(&project.repo_root, &project.project_id)
            .expect("initialize workspace head");
    assert_eq!(
        initial_head.project_id.as_str(),
        project.project_id.as_str()
    );
    assert_eq!(initial_head.head_id, None);
    assert_eq!(initial_head.tree_id, None);
    assert_eq!(initial_head.workspace_epoch, 0);
    assert_eq!(initial_head.latest_history_operation_id, None);

    let advanced = project_store::advance_code_workspace_epoch(
        &project.repo_root,
        &AdvanceCodeWorkspaceEpochRequest {
            project_id: project.project_id.clone(),
            head_id: Some("code-commit-1".into()),
            tree_id: Some("code-tree-1".into()),
            commit_id: Some("code-commit-1".into()),
            latest_history_operation_id: Some("history-op-1".into()),
            affected_paths: vec![
                "src/lib.rs".into(),
                "src/main.rs".into(),
                "src/lib.rs".into(),
            ],
            updated_at: "2026-05-06T12:00:00Z".into(),
        },
    )
    .expect("advance workspace epoch");

    assert_eq!(
        advanced.workspace_head.head_id.as_deref(),
        Some("code-commit-1")
    );
    assert_eq!(
        advanced.workspace_head.tree_id.as_deref(),
        Some("code-tree-1")
    );
    assert_eq!(advanced.workspace_head.workspace_epoch, 1);
    assert_eq!(
        advanced
            .workspace_head
            .latest_history_operation_id
            .as_deref(),
        Some("history-op-1")
    );
    assert_eq!(
        advanced
            .path_epochs
            .iter()
            .map(|epoch| epoch.path.as_str())
            .collect::<Vec<_>>(),
        vec!["src/lib.rs", "src/main.rs"]
    );
    assert!(advanced
        .path_epochs
        .iter()
        .all(|epoch| epoch.workspace_epoch == 1
            && epoch.commit_id.as_deref() == Some("code-commit-1")
            && epoch.history_operation_id.as_deref() == Some("history-op-1")));

    let lib_epoch =
        project_store::read_code_path_epoch(&project.repo_root, &project.project_id, "src/lib.rs")
            .expect("read path epoch")
            .expect("path epoch exists");
    assert_eq!(lib_epoch.workspace_epoch, 1);

    let reopened = Connection::open(database_path_for_repo(&project.repo_root))
        .expect("reopen app-data database");
    let raw_head: (Option<String>, Option<String>, i64, Option<String>) = reopened
        .query_row(
            r#"
            SELECT head_id, tree_id, workspace_epoch, latest_history_operation_id
            FROM code_workspace_heads
            WHERE project_id = ?1
            "#,
            params![project.project_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .expect("reload workspace head from reopened store");
    assert_eq!(
        raw_head,
        (
            Some("code-commit-1".into()),
            Some("code-tree-1".into()),
            1,
            Some("history-op-1".into())
        )
    );

    let path_epoch_count: i64 = reopened
        .query_row(
            "SELECT COUNT(*) FROM code_path_epochs WHERE project_id = ?1 AND workspace_epoch = 1",
            params![project.project_id.as_str()],
            |row| row.get(0),
        )
        .expect("count path epochs");
    assert_eq!(path_epoch_count, 2);

    let reloaded_head =
        project_store::read_code_workspace_head(&project.repo_root, &project.project_id)
            .expect("read workspace head")
            .expect("workspace head exists");
    assert_eq!(reloaded_head, advanced.workspace_head);
}

fn seed_project(project_id: &str) -> TestProject {
    let guard = PROJECT_DB_LOCK.lock().expect("project db lock");
    let tempdir = tempdir().expect("tempdir");
    let app_data_dir = tempdir.path().join("app-data");
    let repo_root = tempdir.path().join("repo");
    fs::create_dir_all(repo_root.join("src")).expect("repo root");
    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let repository = CanonicalRepository {
        project_id: project_id.into(),
        repository_id: format!("repo-{project_id}"),
        root_path: canonical_root.clone(),
        root_path_string: canonical_root.to_string_lossy().into_owned(),
        common_git_dir: canonical_root.join(".git"),
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
    };

    db::configure_project_database_paths(&app_data_dir.join("global.db"));
    let state = DesktopState::default();
    db::import_project(&repository, state.import_failpoints()).expect("import project");

    TestProject {
        _tempdir: tempdir,
        repo_root: canonical_root,
        project_id: project_id.into(),
        _guard: guard,
    }
}
