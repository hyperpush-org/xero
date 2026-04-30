use std::{
    fs,
    path::{Path, PathBuf},
};

use git2::{IndexAddOption, Repository, Signature};
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        get_repository_diff, get_repository_status, import_repository, CommandError,
        CommandErrorClass, ImportRepositoryRequestDto, ProjectIdRequestDto,
        RepositoryDiffRequestDto, RepositoryDiffResponseDto, RepositoryDiffScope,
        RepositoryStatusResponseDto,
    },
    configure_builder_with_state,
    git::{
        diff::{load_repository_diff_from_root, MAX_PATCH_BYTES},
        status::load_repository_status_from_root,
    },
    state::DesktopState,
};

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn registry_path(root: &TempDir) -> PathBuf {
    root.path().join("app-data").join("xero.db")
}

fn create_state(registry_root: &TempDir) -> DesktopState {
    DesktopState::default().with_global_db_path_override(registry_path(registry_root))
}

fn import_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    path: impl AsRef<Path>,
) -> Result<xero_desktop_lib::commands::ImportRepositoryResponseDto, CommandError> {
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
) -> Result<xero_desktop_lib::commands::RepositoryStatusResponseDto, CommandError> {
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
) -> Result<xero_desktop_lib::commands::RepositoryDiffResponseDto, CommandError> {
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

    fs::write(temp_dir.path().join("README.md"), "Xero\n").expect("write README");
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
    let signature = Signature::now("Xero", "Xero@example.com").expect("signature");

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

fn configure_origin_upstream(repository: &Repository, remote_url: &str, branch_name: &str) {
    repository
        .remote("origin", remote_url)
        .expect("create origin remote");
    let mut remote = repository.find_remote("origin").expect("find origin");
    remote
        .push(
            &[format!("refs/heads/{branch_name}:refs/heads/{branch_name}")],
            None,
        )
        .expect("push initial branch");

    let mut config = repository.config().expect("repo config");
    config
        .set_str(&format!("branch.{branch_name}.remote"), "origin")
        .expect("set branch remote");
    config
        .set_str(
            &format!("branch.{branch_name}.merge"),
            &format!("refs/heads/{branch_name}"),
        )
        .expect("set branch merge target");
}

fn push_origin_branch(repository: &Repository, branch_name: &str) {
    let mut remote = repository.find_remote("origin").expect("find origin");
    remote
        .push(
            &[format!("refs/heads/{branch_name}:refs/heads/{branch_name}")],
            None,
        )
        .expect("push branch");
}

fn fetch_origin_branch(repository: &Repository, branch_name: &str) {
    let mut remote = repository.find_remote("origin").expect("find origin");
    remote
        .fetch(&[branch_name], None, None)
        .expect("fetch branch");
}

fn assert_status_matches_root(repository_root: &Path, status: &RepositoryStatusResponseDto) {
    let root_status =
        load_repository_status_from_root(repository_root).expect("load root git status projection");
    assert_eq!(status, &root_status);
}

fn assert_diff_matches_root(
    repository_root: &Path,
    diff: &RepositoryDiffResponseDto,
    scope: RepositoryDiffScope,
) {
    let root_diff = load_repository_diff_from_root(repository_root, scope)
        .expect("load root git diff projection");
    assert_eq!(diff, &root_diff.response);
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
    assert_status_matches_root(repository_root.path(), &status);

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
    assert_diff_matches_root(
        repository_root.path(),
        &staged_diff,
        RepositoryDiffScope::Staged,
    );
    assert_diff_matches_root(
        repository_root.path(),
        &unstaged_diff,
        RepositoryDiffScope::Unstaged,
    );
    assert_diff_matches_root(
        repository_root.path(),
        &worktree_diff,
        RepositoryDiffScope::Worktree,
    );
}

#[test]
fn repository_status_and_diffs_surface_real_staged_unstaged_and_untracked_truth() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let repository = Repository::open(repository_root.path()).expect("open repository");
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");

    fs::write(repository_root.path().join("README.md"), "updated\n").expect("modify tracked file");
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
            && entry.unstaged == Some(xero_desktop_lib::commands::ChangeKind::Modified)
    }));
    assert!(status.entries.iter().any(|entry| {
        entry.path == "staged.txt"
            && entry.staged == Some(xero_desktop_lib::commands::ChangeKind::Added)
    }));
    assert!(status
        .entries
        .iter()
        .any(|entry| entry.path == "untracked.txt" && entry.untracked));
    assert_status_matches_root(repository_root.path(), &status);

    let staged_diff = get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Staged)
        .expect("staged diff succeeds");
    assert!(!staged_diff.patch.is_empty());
    assert!(staged_diff.patch.contains("staged.txt"));
    assert!(staged_diff.patch.contains("+staged change"));

    let unstaged_diff =
        get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Unstaged)
            .expect("unstaged diff succeeds");
    assert!(!unstaged_diff.patch.is_empty());
    assert!(unstaged_diff.patch.contains("README.md"));
    assert!(unstaged_diff.patch.contains("-Xero"));
    assert!(unstaged_diff.patch.contains("+updated"));

    let worktree_diff =
        get_diff_with_app(&app, &imported.project.id, RepositoryDiffScope::Worktree)
            .expect("worktree diff succeeds");
    assert!(worktree_diff.patch.contains("README.md"));
    assert!(worktree_diff.patch.contains("staged.txt"));
    assert!(worktree_diff.patch.contains("-Xero"));
    assert!(worktree_diff.patch.contains("+updated"));
    assert!(worktree_diff.patch.contains("+staged change"));
    assert_diff_matches_root(
        repository_root.path(),
        &staged_diff,
        RepositoryDiffScope::Staged,
    );
    assert_diff_matches_root(
        repository_root.path(),
        &unstaged_diff,
        RepositoryDiffScope::Unstaged,
    );
    assert_diff_matches_root(
        repository_root.path(),
        &worktree_diff,
        RepositoryDiffScope::Worktree,
    );
}

#[test]
fn repository_status_surfaces_real_upstream_ahead_behind_counts() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let remote_root = tempfile::tempdir().expect("remote temp dir");
    let remote_repository = Repository::init_bare(remote_root.path()).expect("bare git repo");
    let repository = Repository::open(repository_root.path()).expect("open repository");
    let branch_name = current_branch_name(&repository).expect("current branch");
    let remote_url = remote_root.path().to_string_lossy().into_owned();

    configure_origin_upstream(&repository, &remote_url, &branch_name);
    remote_repository
        .set_head(&format!("refs/heads/{branch_name}"))
        .expect("point bare HEAD at pushed branch");

    fs::write(repository_root.path().join("local.txt"), "local change\n")
        .expect("write local fixture");
    commit_all(&repository, "local commit");

    let remote_worktree = tempfile::tempdir().expect("remote worktree temp dir");
    let remote_clone =
        Repository::clone(&remote_url, remote_worktree.path()).expect("clone remote");
    fs::write(remote_worktree.path().join("remote.txt"), "remote change\n")
        .expect("write remote fixture");
    commit_all(&remote_clone, "remote commit");
    push_origin_branch(&remote_clone, &branch_name);

    fetch_origin_branch(&repository, &branch_name);

    let app = build_mock_app(create_state(&registry_root));
    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let status = get_status_with_app(&app, &imported.project.id).expect("status succeeds");
    let branch = status.branch.as_ref().expect("branch summary");
    let upstream = branch.upstream.as_ref().expect("upstream summary");

    assert_eq!(branch.name, branch_name);
    assert_eq!(upstream.name, format!("origin/{branch_name}"));
    assert_eq!(upstream.ahead, 1);
    assert_eq!(upstream.behind, 1);
    assert_status_matches_root(repository_root.path(), &status);
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

    let branch = status.branch.as_ref().expect("detached head summary");
    let head_sha = head_oid.to_string();
    assert!(branch.detached);
    assert_eq!(branch.name, "HEAD");
    assert_eq!(branch.head_sha.as_deref(), Some(head_sha.as_str()));
    assert_status_matches_root(repository_root.path(), &status);
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
    assert_diff_matches_root(repository_root.path(), &diff, RepositoryDiffScope::Staged);
}
