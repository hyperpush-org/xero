use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Mutex},
};

use cadence_desktop_lib::{
    commands::{
        get_project_snapshot, import_repository, list_projects, CommandError, CommandErrorClass,
        ImportRepositoryRequestDto, ListProjectsResponseDto, PhaseStatus, PhaseStep,
        PhaseSummaryDto, ProjectIdRequestDto, ProjectSnapshotResponseDto, ProjectUpdateReason,
        ProjectUpdatedPayloadDto, RepositoryStatusChangedPayloadDto, PROJECT_UPDATED_EVENT,
        REPOSITORY_STATUS_CHANGED_EVENT,
    },
    configure_builder_with_state,
    db::migrations::migrations,
    registry::{self, ProjectRegistry, RegistryProjectRecord},
    state::{DesktopState, ImportFailpoints},
};
use git2::{IndexAddOption, Repository, Signature, StatusOptions};
use rusqlite::{params, Connection};
use tauri::{Listener, Manager};
use tempfile::TempDir;

#[derive(Clone, Default)]
struct EventRecorder {
    project_updates: Arc<Mutex<Vec<ProjectUpdatedPayloadDto>>>,
    repository_status_updates: Arc<Mutex<Vec<RepositoryStatusChangedPayloadDto>>>,
}

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn attach_event_recorders(app: &tauri::App<tauri::test::MockRuntime>) -> EventRecorder {
    let recorder = EventRecorder::default();

    let project_updates = Arc::clone(&recorder.project_updates);
    app.listen(PROJECT_UPDATED_EVENT, move |event| {
        let payload: ProjectUpdatedPayloadDto = serde_json::from_str(event.payload())
            .expect("project.updated payload should deserialize");
        project_updates
            .lock()
            .expect("project updates lock")
            .push(payload);
    });

    let repository_status_updates = Arc::clone(&recorder.repository_status_updates);
    app.listen(REPOSITORY_STATUS_CHANGED_EVENT, move |event| {
        let payload: RepositoryStatusChangedPayloadDto = serde_json::from_str(event.payload())
            .expect("repository.status_changed payload should deserialize");
        repository_status_updates
            .lock()
            .expect("repository status updates lock")
            .push(payload);
    });

    recorder
}

fn registry_path(root: &TempDir) -> PathBuf {
    root.path().join("app-data").join("project-registry.json")
}

fn create_state(registry_root: &TempDir) -> DesktopState {
    DesktopState::default().with_registry_file_override(registry_path(registry_root))
}

fn import_with_raw_path(
    app: &tauri::App<tauri::test::MockRuntime>,
    path: &str,
) -> Result<cadence_desktop_lib::commands::ImportRepositoryResponseDto, CommandError> {
    import_repository(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ImportRepositoryRequestDto {
            path: path.to_owned(),
        },
    )
}

fn import_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    path: impl AsRef<Path>,
) -> Result<cadence_desktop_lib::commands::ImportRepositoryResponseDto, CommandError> {
    import_with_raw_path(app, &path.as_ref().to_string_lossy())
}

fn list_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
) -> Result<ListProjectsResponseDto, CommandError> {
    list_projects(app.handle().clone(), app.state::<DesktopState>())
}

fn snapshot_with_app(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
) -> Result<ProjectSnapshotResponseDto, CommandError> {
    get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.to_owned(),
        },
    )
}

fn assert_summary_counts_match_snapshot(
    listed_project: &cadence_desktop_lib::commands::ProjectSummaryDto,
    snapshot: &ProjectSnapshotResponseDto,
) {
    assert_eq!(listed_project.id, snapshot.project.id);
    assert_eq!(listed_project.total_phases, snapshot.project.total_phases);
    assert_eq!(
        listed_project.completed_phases,
        snapshot.project.completed_phases
    );
    assert_eq!(listed_project.active_phase, snapshot.project.active_phase);
}

fn init_git_repo() -> TempDir {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let repository = Repository::init(temp_dir.path()).expect("git repo");

    fs::write(temp_dir.path().join("README.md"), "cadence\n").expect("write README");
    commit_all(&repository, "initial commit");

    temp_dir
}

fn init_git_worktree() -> (TempDir, PathBuf, PathBuf) {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let main_repo_root = temp_dir.path().join("main-repo");
    let worktree_root = temp_dir.path().join("linked-worktree");
    let repository = Repository::init(&main_repo_root).expect("git repo");

    fs::write(main_repo_root.join("README.md"), "cadence\n").expect("write README");
    commit_all(&repository, "initial commit");

    let output = Command::new("git")
        .arg("-C")
        .arg(&main_repo_root)
        .arg("worktree")
        .arg("add")
        .arg("-b")
        .arg("cadence-worktree")
        .arg(&worktree_root)
        .output()
        .expect("git worktree add");
    assert!(
        output.status.success(),
        "git worktree add failed: stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    (temp_dir, main_repo_root, worktree_root)
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

fn read_registry(path: &Path) -> ProjectRegistry {
    let contents = fs::read_to_string(path).expect("read registry");
    serde_json::from_str(&contents).expect("parse registry")
}

fn database_path(repo_root: &Path) -> PathBuf {
    repo_root.join(".cadence").join("state.db")
}

fn common_exclude_path(repo_root: &Path) -> PathBuf {
    let repository = Repository::open(repo_root).expect("open repository");
    repository.commondir().join("info").join("exclude")
}

fn assert_database_rows(repo_root: &Path, project_id: &str, repository_id: &str, root_path: &str) {
    let connection = Connection::open(database_path(repo_root)).expect("open sqlite db");
    let project_row: (String, String, String, i64, i64, i64, Option<String>) = connection
        .query_row(
            "SELECT id, name, milestone, total_phases, completed_phases, active_phase, branch FROM projects WHERE id = ?1",
            [project_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .expect("project row");
    assert_eq!(project_row.0, project_id);
    assert_eq!(
        project_row.1,
        repo_root.file_name().unwrap().to_string_lossy()
    );
    assert_eq!(project_row.2, "");
    assert_eq!(project_row.3, 0);
    assert_eq!(project_row.4, 0);
    assert_eq!(project_row.5, 0);
    assert!(
        project_row.6.is_some(),
        "import should persist the current branch name"
    );

    let repository_row: (String, String, String, String, bool) = connection
        .query_row(
            "SELECT id, project_id, root_path, display_name, is_git_repo FROM repositories WHERE id = ?1",
            [repository_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get::<_, i64>(4)? == 1,
                ))
            },
        )
        .expect("repository row");
    assert_eq!(repository_row.0, repository_id);
    assert_eq!(repository_row.1, project_id);
    assert_eq!(repository_row.2, root_path);
    assert_eq!(
        repository_row.3,
        repo_root.file_name().unwrap().to_string_lossy()
    );
    assert!(repository_row.4);
}

#[derive(Debug, Clone, Copy)]
struct PhaseRowFixture<'a> {
    project_id: &'a str,
    id: u32,
    name: &'a str,
    description: &'a str,
    status: &'a str,
    current_step: Option<&'a str>,
    task_count: u32,
    completed_tasks: u32,
    summary: Option<&'a str>,
}

fn open_state_connection(repo_root: &Path) -> Connection {
    let connection = Connection::open(database_path(repo_root)).expect("open sqlite db");
    connection
        .execute_batch("PRAGMA foreign_keys = ON;")
        .expect("enable foreign keys");
    connection
}

fn insert_project_fixture(repo_root: &Path, project_id: &str, name: &str) {
    let connection = open_state_connection(repo_root);
    connection
        .execute(
            r#"
            INSERT INTO projects (
                id,
                name,
                description,
                milestone,
                total_phases,
                completed_phases,
                active_phase,
                branch,
                runtime,
                updated_at
            )
            VALUES (?1, ?2, '', '', 0, 0, 0, NULL, NULL, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            "#,
            params![project_id, name],
        )
        .expect("insert project fixture");
}

fn insert_phase_rows(repo_root: &Path, phase_rows: &[PhaseRowFixture<'_>]) {
    let connection = open_state_connection(repo_root);

    for phase in phase_rows {
        connection
            .execute(
                r#"
                INSERT INTO workflow_phases (
                    project_id,
                    id,
                    name,
                    description,
                    status,
                    current_step,
                    task_count,
                    completed_tasks,
                    summary,
                    updated_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
                "#,
                params![
                    phase.project_id,
                    phase.id,
                    phase.name,
                    phase.description,
                    phase.status,
                    phase.current_step,
                    phase.task_count,
                    phase.completed_tasks,
                    phase.summary,
                ],
            )
            .expect("insert phase fixture");
    }
}

fn overwrite_project_summary_counts(
    repo_root: &Path,
    project_id: &str,
    total_phases: u32,
    completed_phases: u32,
    active_phase: u32,
) {
    let connection = open_state_connection(repo_root);
    connection
        .execute(
            r#"
            UPDATE projects
            SET total_phases = ?2,
                completed_phases = ?3,
                active_phase = ?4,
                updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
            WHERE id = ?1
            "#,
            params![project_id, total_phases, completed_phases, active_phase],
        )
        .expect("overwrite project summary counts");
}

fn repository_status_paths(repo_root: &Path) -> Vec<String> {
    let repository = Repository::open(repo_root).expect("open repository");
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false)
        .include_unmodified(false);

    let statuses = repository
        .statuses(Some(&mut options))
        .expect("read git status");

    statuses
        .iter()
        .filter_map(|entry| entry.path().map(ToOwned::to_owned))
        .collect()
}

#[test]
fn import_repository_bootstraps_repo_local_state_and_registry_idempotently() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let app = build_mock_app(create_state(&registry_root));
    let recorder = attach_event_recorders(&app);

    let first = import_with_app(&app, repository_root.path()).expect("first import succeeds");
    let second = import_with_app(&app, repository_root.path()).expect("second import succeeds");

    assert_eq!(first.project.id, second.project.id);
    assert_eq!(first.repository.id, second.repository.id);
    assert_eq!(first.repository.root_path, second.repository.root_path);
    assert_eq!(
        first.project.name,
        repository_root
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
    );
    assert!(database_path(repository_root.path()).exists());

    let exclude_contents =
        fs::read_to_string(common_exclude_path(repository_root.path())).expect("read git exclude");
    let cadence_entries = exclude_contents
        .lines()
        .filter(|line| line.trim() == ".cadence/")
        .count();
    assert_eq!(
        cadence_entries, 1,
        "cadence exclude entry should be deduplicated"
    );

    let registry = read_registry(&registry_path(&registry_root));
    assert_eq!(registry.version, 1);
    assert_eq!(registry.projects.len(), 1);
    assert_eq!(registry.projects[0].project_id, first.project.id);
    assert_eq!(registry.projects[0].repository_id, first.repository.id);
    assert_eq!(registry.projects[0].root_path, first.repository.root_path);

    assert_database_rows(
        repository_root.path(),
        &first.project.id,
        &first.repository.id,
        &first.repository.root_path,
    );

    let git_status_paths = repository_status_paths(repository_root.path());
    assert!(
        git_status_paths.is_empty(),
        "repo should stay clean after import, found statuses: {git_status_paths:?}"
    );

    let project_updates = recorder
        .project_updates
        .lock()
        .expect("project updates lock");
    assert_eq!(project_updates.len(), 2);
    assert!(project_updates
        .iter()
        .all(|payload| payload.reason == ProjectUpdateReason::Imported));
    assert!(project_updates
        .iter()
        .all(|payload| payload.project.id == first.project.id));

    let repository_status_updates = recorder
        .repository_status_updates
        .lock()
        .expect("repository status updates lock");
    assert_eq!(repository_status_updates.len(), 2);
    assert!(repository_status_updates
        .iter()
        .all(|payload| payload.project_id == first.project.id
            && payload.repository_id == first.repository.id));
    assert!(repository_status_updates
        .iter()
        .all(|payload| payload.status.entries.is_empty()));
}

#[test]
fn import_repository_canonicalizes_nested_and_symlinked_paths_to_one_repo() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let nested_dir = repository_root.path().join("nested").join("deeper");
    fs::create_dir_all(&nested_dir).expect("nested dir");

    #[cfg(unix)]
    let symlink_path = {
        let symlink_path = repository_root.path().join("nested-link");
        std::os::unix::fs::symlink(&nested_dir, &symlink_path).expect("symlink nested dir");
        symlink_path
    };

    #[cfg(not(unix))]
    let symlink_path = nested_dir.clone();

    let app = build_mock_app(create_state(&registry_root));

    let nested_import = import_with_app(&app, &nested_dir).expect("nested import succeeds");
    let symlink_import = import_with_app(&app, &symlink_path).expect("symlink import succeeds");

    assert_eq!(nested_import.project.id, symlink_import.project.id);
    assert_eq!(nested_import.repository.id, symlink_import.repository.id);
    assert_eq!(
        nested_import.repository.root_path,
        symlink_import.repository.root_path
    );

    let canonical_root = fs::canonicalize(repository_root.path()).expect("canonical repo root");
    assert_eq!(
        nested_import.repository.root_path,
        canonical_root.to_string_lossy().into_owned()
    );

    let registry = read_registry(&registry_path(&registry_root));
    assert_eq!(registry.projects.len(), 1);
}

#[test]
fn import_repository_rejects_empty_missing_and_non_git_paths_without_creating_state() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let app = build_mock_app(create_state(&registry_root));

    let empty_error = import_with_raw_path(&app, "   ").expect_err("empty path should fail");
    assert_eq!(empty_error.code, "invalid_request");
    assert_eq!(empty_error.class, CommandErrorClass::UserFixable);
    assert!(!registry_path(&registry_root).exists());

    let missing_path = registry_root.path().join("missing-repo");

    let missing_error = import_with_app(&app, &missing_path).expect_err("missing path should fail");
    assert_eq!(missing_error.code, "repository_path_not_found");
    assert_eq!(missing_error.class, CommandErrorClass::UserFixable);
    assert!(!missing_path.join(".cadence").exists());

    let non_git_dir = tempfile::tempdir().expect("non git dir");
    let non_git_error =
        import_with_app(&app, non_git_dir.path()).expect_err("non-git path should fail");
    assert_eq!(non_git_error.code, "git_repository_not_found");
    assert!(!non_git_dir.path().join(".cadence").exists());
    assert!(!registry_path(&registry_root).exists());
}

#[test]
fn import_repository_surfaces_exclude_migration_and_registry_failures() {
    let exclude_failure_root = tempfile::tempdir().expect("registry temp dir");
    let exclude_repo = init_git_repo();
    let exclude_app = build_mock_app(create_state(&exclude_failure_root).with_failpoints(
        ImportFailpoints {
            fail_exclude_write: true,
            ..ImportFailpoints::default()
        },
    ));

    let exclude_error = import_with_app(&exclude_app, exclude_repo.path())
        .expect_err("exclude failpoint should fail import");
    assert_eq!(exclude_error.code, "git_exclude_write_failed");
    assert!(!database_path(exclude_repo.path()).exists());
    assert!(!registry_path(&exclude_failure_root).exists());

    let migration_registry_root = tempfile::tempdir().expect("registry temp dir");
    let migration_repo = init_git_repo();
    let migration_app = build_mock_app(create_state(&migration_registry_root).with_failpoints(
        ImportFailpoints {
            fail_migration: true,
            ..ImportFailpoints::default()
        },
    ));

    let migration_error = import_with_app(&migration_app, migration_repo.path())
        .expect_err("migration failpoint should fail import");
    assert_eq!(migration_error.code, "state_database_migration_failed");
    assert!(!database_path(migration_repo.path()).exists());
    assert!(!registry_path(&migration_registry_root).exists());

    let registry_failure_root = tempfile::tempdir().expect("registry temp dir");
    let registry_repo = init_git_repo();
    let registry_app = build_mock_app(create_state(&registry_failure_root).with_failpoints(
        ImportFailpoints {
            fail_registry_write: true,
            ..ImportFailpoints::default()
        },
    ));

    let registry_error = import_with_app(&registry_app, registry_repo.path())
        .expect_err("registry failpoint should fail import");
    assert_eq!(registry_error.code, "registry_write_failed");
    assert!(
        database_path(registry_repo.path()).exists(),
        "repo-local db should exist even when registry persistence fails"
    );
    assert!(!registry_path(&registry_failure_root).exists());
}

#[test]
fn import_repository_reuses_preexisting_repo_local_database() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let cadence_dir = repository_root.path().join(".cadence");
    fs::create_dir_all(&cadence_dir).expect("create cadence dir");

    let mut connection =
        Connection::open(database_path(repository_root.path())).expect("open sqlite db");
    migrations()
        .to_latest(&mut connection)
        .expect("migrate preexisting database");
    connection
        .execute(
            "CREATE TABLE IF NOT EXISTS sentinel (value TEXT NOT NULL)",
            [],
        )
        .expect("create sentinel table");
    connection
        .execute("INSERT INTO sentinel (value) VALUES ('keep-me')", [])
        .expect("insert sentinel row");
    drop(connection);

    let app = build_mock_app(create_state(&registry_root));
    let response = import_with_app(&app, repository_root.path()).expect("import succeeds");

    assert_database_rows(
        repository_root.path(),
        &response.project.id,
        &response.repository.id,
        &response.repository.root_path,
    );

    let connection =
        Connection::open(database_path(repository_root.path())).expect("open sqlite db");
    let sentinel: String = connection
        .query_row("SELECT value FROM sentinel LIMIT 1", [], |row| row.get(0))
        .expect("sentinel row should survive import");
    assert_eq!(sentinel, "keep-me");

    let graph_tables: Vec<String> = connection
        .prepare(
            r#"
            SELECT name
            FROM sqlite_master
            WHERE type = 'table'
              AND name IN (
                'workflow_graph_nodes',
                'workflow_graph_edges',
                'workflow_gate_metadata',
                'workflow_transition_events'
              )
            ORDER BY name ASC
            "#,
        )
        .expect("prepare graph table lookup")
        .query_map([], |row| row.get(0))
        .expect("query graph table lookup")
        .collect::<Result<Vec<_>, _>>()
        .expect("collect graph table names");

    assert_eq!(
        graph_tables,
        vec![
            "workflow_gate_metadata".to_string(),
            "workflow_graph_edges".to_string(),
            "workflow_graph_nodes".to_string(),
            "workflow_transition_events".to_string(),
        ]
    );
}

#[test]
fn import_repository_keeps_git_worktrees_clean_via_common_exclude() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let (_workspace_root, main_repo_root, worktree_root) = init_git_worktree();
    let app = build_mock_app(create_state(&registry_root));

    let response = import_with_app(&app, &worktree_root).expect("worktree import succeeds");

    let exclude_contents =
        fs::read_to_string(common_exclude_path(&main_repo_root)).expect("read common exclude");
    let cadence_entries = exclude_contents
        .lines()
        .filter(|line| line.trim() == ".cadence/")
        .count();
    assert_eq!(
        cadence_entries, 1,
        "cadence exclude entry should live in the common git dir"
    );

    let git_status_paths = repository_status_paths(&worktree_root);
    assert!(
        git_status_paths.is_empty(),
        "worktree should stay clean after import, found statuses: {git_status_paths:?}"
    );

    let registry = read_registry(&registry_path(&registry_root));
    assert_eq!(registry.projects.len(), 1);
    assert_eq!(registry.projects[0].project_id, response.project.id);
    assert_eq!(
        registry.projects[0].root_path,
        response.repository.root_path
    );
}

#[test]
fn import_repository_handles_malformed_registry_and_read_only_repo_failures() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let registry_file = registry_path(&registry_root);
    if let Some(parent) = registry_file.parent() {
        fs::create_dir_all(parent).expect("create registry dir");
    }
    fs::write(&registry_file, "{ definitely not json }").expect("write malformed registry");

    let app = build_mock_app(create_state(&registry_root));
    let response = import_with_app(&app, repository_root.path())
        .expect("import should recover malformed registry");
    let recovered_registry = read_registry(&registry_file);
    assert_eq!(recovered_registry.projects.len(), 1);
    assert_eq!(
        recovered_registry.projects[0].project_id,
        response.project.id
    );
    assert!(registry_file.with_extension("json.corrupt").exists());

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let read_only_repo = init_git_repo();
        fs::set_permissions(read_only_repo.path(), fs::Permissions::from_mode(0o555))
            .expect("make repo root read-only");

        let read_only_registry_root = tempfile::tempdir().expect("registry temp dir");
        let read_only_app = build_mock_app(create_state(&read_only_registry_root));
        let read_only_error = import_with_app(&read_only_app, read_only_repo.path())
            .expect_err("read-only repo should fail import");

        fs::set_permissions(read_only_repo.path(), fs::Permissions::from_mode(0o755))
            .expect("restore repo permissions");

        assert_eq!(read_only_error.code, "cadence_state_dir_unavailable");
        assert!(!registry_path(&read_only_registry_root).exists());
    }
}

#[test]
fn list_projects_reopens_valid_imports_and_prunes_deleted_roots() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let app = build_mock_app(create_state(&registry_root));
    let primary_repo = init_git_repo();
    let deleted_repo = init_git_repo();

    let primary_import =
        import_with_app(&app, primary_repo.path()).expect("primary import succeeds");
    let deleted_import =
        import_with_app(&app, deleted_repo.path()).expect("secondary import succeeds");

    let registry_file = registry_path(&registry_root);
    let duplicate_record = RegistryProjectRecord {
        project_id: primary_import.project.id.clone(),
        repository_id: primary_import.repository.id.clone(),
        root_path: primary_import.repository.root_path.clone(),
    };
    let malformed_record = serde_json::json!({
        "projectId": "malformed",
        "rootPath": primary_import.repository.root_path,
    });

    fs::write(
        &registry_file,
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 1,
            "projects": [
                duplicate_record,
                read_registry(&registry_file).projects[0].clone(),
                read_registry(&registry_file).projects[1].clone(),
                malformed_record,
            ],
        }))
        .expect("serialize registry fixture"),
    )
    .expect("write registry fixture");

    fs::remove_dir_all(deleted_repo.path()).expect("delete imported repo root");
    overwrite_project_summary_counts(primary_repo.path(), &primary_import.project.id, 9, 4, 3);

    let response = list_with_app(&app).expect("list projects succeeds");
    assert_eq!(response.projects.len(), 1);
    assert_eq!(response.projects[0].id, primary_import.project.id);
    assert_eq!(response.projects[0].runtime, None);

    let snapshot = snapshot_with_app(&app, &primary_import.project.id).expect("snapshot succeeds");
    assert!(snapshot.phases.is_empty());
    assert!(
        snapshot.lifecycle.stages.is_empty(),
        "lifecycle projection should stay empty when no workflow graph rows exist"
    );
    assert_summary_counts_match_snapshot(&response.projects[0], &snapshot);
    assert_eq!(response.projects[0].total_phases, 0);
    assert_eq!(response.projects[0].completed_phases, 0);
    assert_eq!(response.projects[0].active_phase, 0);

    let pruned_registry = registry::read_registry(&registry_file).expect("read pruned registry");
    assert!(pruned_registry
        .projects
        .iter()
        .all(|record| record.root_path != deleted_import.repository.root_path));
}

#[test]
fn get_project_snapshot_projects_seeded_phase_rows_into_ordered_dtos() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    insert_project_fixture(repository_root.path(), "project-shadow", "shadow");
    insert_phase_rows(
        repository_root.path(),
        &[
            PhaseRowFixture {
                project_id: &imported.project.id,
                id: 2,
                name: "Execute shell",
                description: "Project repo-local phases through the shell.",
                status: "active",
                current_step: Some("execute"),
                task_count: 4,
                completed_tasks: 1,
                summary: None,
            },
            PhaseRowFixture {
                project_id: &imported.project.id,
                id: 1,
                name: "Plan workflow",
                description: "Capture the durable workflow projection.",
                status: "complete",
                current_step: Some("ship"),
                task_count: 3,
                completed_tasks: 3,
                summary: Some("Planned and recorded."),
            },
            PhaseRowFixture {
                project_id: "project-shadow",
                id: 1,
                name: "Shadow phase",
                description: "Should never leak into the selected snapshot.",
                status: "blocked",
                current_step: Some("verify"),
                task_count: 9,
                completed_tasks: 0,
                summary: Some("Ignore me."),
            },
        ],
    );
    overwrite_project_summary_counts(repository_root.path(), &imported.project.id, 99, 88, 77);

    let snapshot = snapshot_with_app(&app, &imported.project.id).expect("snapshot succeeds");
    let list_response = list_with_app(&app).expect("list projects succeeds");
    assert_eq!(list_response.projects.len(), 1);
    assert_summary_counts_match_snapshot(&list_response.projects[0], &snapshot);
    assert_eq!(list_response.projects[0].total_phases, 2);
    assert_eq!(list_response.projects[0].completed_phases, 1);
    assert_eq!(list_response.projects[0].active_phase, 2);
    assert_eq!(snapshot.project.total_phases, 2);
    assert_eq!(snapshot.project.completed_phases, 1);
    assert_eq!(snapshot.project.active_phase, 2);
    assert!(
        snapshot.lifecycle.stages.is_empty(),
        "legacy workflow_phases rows should not fabricate planning lifecycle projection"
    );
    assert_eq!(snapshot.project.id, imported.project.id);
    assert_eq!(
        snapshot
            .repository
            .as_ref()
            .map(|repository| repository.id.as_str()),
        Some(imported.repository.id.as_str())
    );
    assert_eq!(
        snapshot.phases,
        vec![
            PhaseSummaryDto {
                id: 1,
                name: "Plan workflow".into(),
                description: "Capture the durable workflow projection.".into(),
                status: PhaseStatus::Complete,
                current_step: Some(PhaseStep::Ship),
                task_count: 3,
                completed_tasks: 3,
                summary: Some("Planned and recorded.".into()),
            },
            PhaseSummaryDto {
                id: 2,
                name: "Execute shell".into(),
                description: "Project repo-local phases through the shell.".into(),
                status: PhaseStatus::Active,
                current_step: Some(PhaseStep::Execute),
                task_count: 4,
                completed_tasks: 1,
                summary: None,
            },
        ]
    );
}

#[test]
fn get_project_snapshot_surfaces_unknown_phase_status_as_schema_drift() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    insert_phase_rows(
        repository_root.path(),
        &[PhaseRowFixture {
            project_id: &imported.project.id,
            id: 1,
            name: "Broken phase",
            description: "Status text should decode strictly.",
            status: "mystery",
            current_step: Some("plan"),
            task_count: 1,
            completed_tasks: 0,
            summary: None,
        }],
    );

    let error = snapshot_with_app(&app, &imported.project.id)
        .expect_err("snapshot should fail on unknown phase status");
    assert_eq!(error.code, "project_phase_decode_failed");
    assert_eq!(error.class, CommandErrorClass::SystemFault);
    assert!(error.message.contains("Unknown phase status `mystery`."));
}

#[test]
fn get_project_snapshot_surfaces_unknown_phase_step_as_schema_drift() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    insert_phase_rows(
        repository_root.path(),
        &[PhaseRowFixture {
            project_id: &imported.project.id,
            id: 1,
            name: "Broken step",
            description: "Current step text should decode strictly.",
            status: "active",
            current_step: Some("invent"),
            task_count: 2,
            completed_tasks: 1,
            summary: Some("Unexpected step."),
        }],
    );

    let error = snapshot_with_app(&app, &imported.project.id)
        .expect_err("snapshot should fail on unknown phase current_step");
    assert_eq!(error.code, "project_phase_decode_failed");
    assert_eq!(error.class, CommandErrorClass::SystemFault);
    assert!(error
        .message
        .contains("Unknown phase current_step `invent`."));
}

#[test]
fn get_project_snapshot_returns_truthful_zero_phase_state_and_typed_missing_db_errors() {
    let registry_root = tempfile::tempdir().expect("registry temp dir");
    let repository_root = init_git_repo();
    let app = build_mock_app(create_state(&registry_root));

    let imported = import_with_app(&app, repository_root.path()).expect("import succeeds");
    let snapshot = snapshot_with_app(&app, &imported.project.id).expect("snapshot succeeds");
    assert_eq!(snapshot.project.id, imported.project.id);
    assert_eq!(snapshot.project.runtime, None);
    assert!(snapshot.phases.is_empty());
    assert!(snapshot.lifecycle.stages.is_empty());
    assert_eq!(
        snapshot
            .repository
            .as_ref()
            .map(|repository| repository.id.as_str()),
        Some(imported.repository.id.as_str())
    );

    fs::remove_file(database_path(repository_root.path())).expect("remove repo-local db");

    let missing_db_error = snapshot_with_app(&app, &imported.project.id)
        .expect_err("snapshot should fail when repo-local state db is missing");
    assert_eq!(missing_db_error.code, "project_state_unavailable");
    assert!(missing_db_error
        .message
        .contains(&imported.repository.root_path));

    let unknown_project_error =
        snapshot_with_app(&app, "project_unknown").expect_err("unknown project should fail");
    assert_eq!(unknown_project_error.code, "project_not_found");
}
