use std::{
    fs,
    path::{Path, PathBuf},
};

use cadence_desktop_lib::{
    commands::{
        get_repository_diff, get_repository_status, import_repository, CommandError,
        CommandErrorClass, ImportRepositoryRequestDto, ProjectIdRequestDto,
        RepositoryDiffRequestDto, RepositoryDiffScope,
    },
    configure_builder_with_state,
    git::diff::MAX_PATCH_BYTES,
    state::DesktopState,
};
use git2::{IndexAddOption, Repository, Signature};
use tauri::Manager;
use tempfile::TempDir;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn registry_path(root: &TempDir) -> PathBuf {
    root.path().join("app-data").join("project-registry.json")
}

fn create_state(registry_root: &TempDir) -> DesktopState {
    DesktopState::default().with_registry_file_override(registry_path(registry_root))
}

fn import_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    path: impl AsRef<Path>,
) -> Result<cadence_desktop_lib::commands::ImportRepositoryResponseDto, CommandError> {
    import_repository(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ImportRepositoryRequestDto {
            path: path.as_ref().to_string_lossy().into_owned(),
        },
    )
}

fn get_status_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
) -> Result<cadence_desktop_lib::commands::RepositoryStatusResponseDto, CommandError> {
    get_repository_status(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.to_owned(),
        },
    )
}

fn get_diff_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    scope: RepositoryDiffScope,
) -> Result<cadence_desktop_lib::commands::RepositoryDiffResponseDto, CommandError> {
    get_repository_diff(
        app.handle().clone(),
        app.state::<DesktopState>(),
        RepositoryDiffRequestDto {
            project_id: project_id.to_owned(),
            scope,
        },
    )
}

fn init_git_repo() -> TempDir {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let repository = Repository::init(temp_dir.path()).expect("git repo");

    fs::write(temp_dir.path().join("README.md"), "cadence\n").expect("write README");
    commit_all(&repository, "initial commit");

    temp_dir
}

fn commit_all(repository: &Repository, message: &str) {
    let mut index = repository.index().expect("repo index");
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .expect("stage files");
    index.write().expect("write index");

    let tree_id = index.write_tree().expect("write tree");
    let tree = repository.find_tree(tree_id).expect("find tree");
    let signature = Signature::now("Cadence", "cadence@example.com").expect("signature");

    let parents = repository
        .head()
        .ok()
        .and_then(|head| head.target())
        .and_then(|oid| repository.find_commit(oid).ok())
        .into_iter()
        .collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();

    repository
        .commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .expect("commit");
}

fn stage_path(repository: &Repository, relative_path: &str) {
    let mut index = repository.index().expect("repo index");
    index
        .add_path(Path::new(relative_path))
        .expect("stage path");
    index.write().expect("write index");
}

fn current_branch_name(repository: &Repository) -> Option<String> {
    repository
        .head()
        .ok()
        .and_then(|head| head.shorthand().map(ToOwned::to_owned))
}

fn current_head_sha(repository: &Repository) -> Option<String> {
    repository
        .head()
        .ok()
        .and_then(|head| head.target().map(|oid| oid.to_string()))
}

#[test]
fn clean_repo_status_and_empty_diffs_are_truthful() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let repository = Repository::open(repository_root.path()).expect("open repository");
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let status = get_status_with_app(&app, &imported.project.id).expect("status succeeds");

    assert_eq!(status.repository.id, imported.repository.id);
    assert_eq!(status.repository.project_id, imported.project.id);
    assert_eq!(
        status.branch.as_ref().map(|branch| branch.name.clone()),
        current_branch_name(&repository)
    );
    assert_eq!(
        status
            .branch
            .as_ref()
            .and_then(|branch| branch.head_sha.clone()),
        current_head_sha(&repository)
    );
    assert!(status.entries.is_empty());
    assert!(!status.has_staged_changes);
    assert!(!status.has_unstaged_changes);
    assert!(!status.has_untracked_changes);

    let staged_diff = get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Staged)
        .expect("staged diff succeeds");
    let unstaged_diff =
        get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Unstaged)
            .expect("unstaged diff succeeds");
    let worktree_diff =
        get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Worktree)
            .expect("worktree diff succeeds");

    assert_eq!(staged_diff.patch, "");
    assert!(!staged_diff.truncated);
    assert_eq!(staged_diff.base_revision, current_head_sha(&repository));
    assert_eq!(unstaged_diff.patch, "");
    assert!(!unstaged_diff.truncated);
    assert_eq!(unstaged_diff.base_revision, None);
    assert_eq!(worktree_diff.patch, "");
    assert!(!worktree_diff.truncated);
    assert_eq!(worktree_diff.base_revision, current_head_sha(&repository));
}

#[test]
fn repository_status_and_diffs_surface_real_staged_unstaged_and_untracked_truth() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let repository = Repository::open(repository_root.path()).expect("open repository");
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");

    fs::write(
        repository_root.path().join("README.md"),
        "cadence\nupdated\n",
    )
    .expect("modify tracked file");
    fs::write(repository_root.path().join("staged.txt"), "staged change\n")
        .expect("write staged file");
    stage_path(&repository, "staged.txt");
    fs::write(
        repository_root.path().join("untracked.txt"),
        "untracked change\n",
    )
    .expect("write untracked file");

    let status = get_status_with_app(&app, &imported.project.id).expect("status succeeds");
    assert!(status.has_staged_changes);
    assert!(status.has_unstaged_changes);
    assert!(status.has_untracked_changes);
    assert!(status.entries.iter().any(|entry| {
        entry.path == "README.md"
            && entry.unstaged == Some(cadence_desktop_lib::commands::ChangeKind::Modified)
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "staged.txt"
            && entry.staged == Some(cadence_desktop_lib::commands::ChangeKind::Added)
    }));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "untracked.txt" && entry.untracked));

    let staged_diff = get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Staged)
        .expect("staged diff succeeds");
    assert!(!staged_diff.patch.is_empty());
    assert!(staged_diff.patch.contains("staged.txt"));

    let unstaged_diff =
        get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Unstaged)
            .expect("unstaged diff succeeds");
    assert!(!unstaged_diff.patch.is_empty());
    assert!(unstaged_diff.patch.contains("README.md"));

    let worktree_diff =
        get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Worktree)
            .expect("worktree diff succeeds");
    assert!(worktree_diff.patch.contains("README.md"));
    assert!(worktree_diff.patch.contains("staged.txt"));
}

#[test]
fn detached_head_status_stays_truthful() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let repository = Repository::open(repository_root.path()).expect("open repository");
    let head_oid = repository.head().expect("head").target().expect("head oid");
    let head_commit = repository.find_commit(head_oid).expect("find commit");
    repository
        .checkout_tree(head_commit.as_object(), None)
        .expect("checkout tree");
    repository.set_head_detached(head_oid).expect("detach head");

    let app = build_mock_app(create_state(&registry_root));
    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let status = get_status_with_app(&app, &imported.project.id).expect("status succeeds");

    let branch = status.branch.expect("detached head summary");
    assert!(branch.detached);
    assert_eq!(branch.name, "HEAD");
    assert_eq!(branch.head_sha, Some(head_oid.to_string()));
}

#[test]
fn repository_commands_fail_cleanly_for_unknown_projects_and_broken_git_state() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let app = build_mock_app(create_state(&registry_root));
    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");

    let missing_project_error =
        get_status_with_app(&app, "project_missing").expect_err("unknown project should fail");
    assert_eq!(missing_project_error.code, "project_not_found");
    assert_eq!(missing_project_error.class, CommandErrorClass::UserFixable);

    let invalid_scope: Result<RepositoryDiffRequestDto, _> =
        serde_json::from_value(serde_json::json!({
            "projectId": imported.project.id,
            "scope": "unsupported"
        }));
    assert!(
        invalid_scope.is_err(),
        "unsupported diff scope should fail request parsing"
    );

    fs::remove_dir_all(repository_root.path().join(".git")).expect("remove git dir");

    let broken_status_error = get_status_with_app(&app, &imported.project.id)
        .expect_err("broken git state should fail status");
    assert_eq!(broken_status_error.code, "git_repository_not_found");

    let broken_diff_error =
        get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Worktree)
            .expect_err("broken git state should fail diff");
    assert_eq!(broken_diff_error.code, "git_repository_not_found");
}

#[test]
fn oversized_diffs_are_truncated_honestly() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let repository = Repository::open(repository_root.path()).expect("open repository");
    let app = build_mock_app(create_state(&registry_root));
    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");

    let large_patch =
        std::iter::repeat_n("line with plenty of diff payload\n", 5_000).collect::<String>();
    fs::write(repository_root.path().join("large.txt"), large_patch)
        .expect("write large patch fixture");
    stage_path(&repository, "large.txt");

    let diff = get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Staged)
        .expect("staged diff succeeds");
    assert!(diff.truncated);
    assert!(diff.patch.len() <= MAX_PATCH_BYTES);
    assert!(diff.patch.contains("large.txt"));
}
