use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use cadence_desktop_lib::{
    auth::{persist_openai_codex_session, StoredOpenAiCodexSession},
    commands::{
        cancel_autonomous_run::cancel_autonomous_run, get_autonomous_run::get_autonomous_run,
        get_runtime_run::get_runtime_run, start_autonomous_run::start_autonomous_run,
        start_runtime_session::start_runtime_session, AutonomousRunRecoveryStateDto,
        AutonomousRunStateDto, AutonomousRunStatusDto, CancelAutonomousRunRequestDto,
        GetAutonomousRunRequestDto, GetRuntimeRunRequestDto, ProjectIdRequestDto,
        RepositoryDiffScope, RuntimeAuthPhase, RuntimeRunDto, RuntimeRunStatusDto,
        RuntimeRunTransportLivenessDto, StartAutonomousRunRequestDto,
    },
    configure_builder_with_state, db,
    git::repository::{ensure_cadence_excluded, CanonicalRepository},
    registry::{self, RegistryProjectRecord},
    runtime::{
        AutonomousCommandRequest, AutonomousEditRequest, AutonomousFindRequest,
        AutonomousGitDiffRequest, AutonomousGitStatusRequest, AutonomousReadRequest,
        AutonomousSkillCacheManifest, AutonomousSkillCacheStatus, AutonomousSkillRuntime,
        AutonomousSkillRuntimeConfig, AutonomousSkillSource, AutonomousSkillSourceEntryKind,
        AutonomousSkillSourceError, AutonomousSkillSourceFileRequest,
        AutonomousSkillSourceFileResponse, AutonomousSkillSourceMetadata,
        AutonomousSkillSourceTreeEntry, AutonomousSkillSourceTreeRequest,
        AutonomousSkillSourceTreeResponse, AutonomousToolOutput, AutonomousToolRuntime,
        AutonomousWriteRequest, FilesystemAutonomousSkillCacheStore,
    },
    state::DesktopState,
};
use git2::{Repository, Status, StatusOptions};
use tauri::Manager;
use tempfile::TempDir;

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

#[path = "support/supervisor_test_lock.rs"]
mod supervisor_test_lock;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> (DesktopState, PathBuf) {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");

    (
        DesktopState::default()
            .with_registry_file_override(registry_path)
            .with_auth_store_file_override(auth_store_path.clone())
            .with_autonomous_skill_cache_dir_override(
                root.path().join("app-data").join("autonomous-skills"),
            )
            .with_runtime_supervisor_binary_override(supervisor_binary_path()),
        auth_store_path,
    )
}

fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_Cadence-runtime-supervisor"))
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

fn commit_all(repo_root: &Path, message: &str) {
    let repo = Repository::open(repo_root).expect("open git repo");
    let mut index = repo.index().expect("open git index");
    index
        .add_all(["*"], git2::IndexAddOption::DEFAULT, None)
        .expect("stage repo contents");
    index.write().expect("write git index");

    let tree_id = index.write_tree().expect("write tree");
    let tree = repo.find_tree(tree_id).expect("find tree");
    let signature =
        git2::Signature::now("Cadence Test", "Cadence@example.com").expect("create test signature");

    let parent = repo.head().ok().and_then(|head| head.peel_to_commit().ok());
    match parent.as_ref() {
        Some(parent) => {
            repo.commit(
                Some("HEAD"),
                &signature,
                &signature,
                message,
                &tree,
                &[parent],
            )
            .expect("commit repo state");
        }
        None => {
            repo.commit(Some("HEAD"), &signature, &signature, message, &tree, &[])
                .expect("create initial commit");
        }
    }
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> (String, PathBuf) {
    let repo_root = root.path().join("imported-proof-repo");
    fs::create_dir_all(repo_root.join("notes")).expect("create repo notes directory");
    fs::write(repo_root.join("README.md"), "alpha\nbeta\n").expect("seed imported repo readme");

    Repository::init(&repo_root).expect("init imported git repo");
    commit_all(&repo_root, "Initial imported proof repo state");

    let canonical_root = fs::canonicalize(&repo_root).expect("canonicalize repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();

    let repository = CanonicalRepository {
        project_id: "project-1".into(),
        repository_id: "repo-1".into(),
        root_path: canonical_root.clone(),
        root_path_string: root_path_string.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "imported-proof-repo".into(),
        branch_name: Some("main".into()),
        head_sha: None,
        branch: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

    ensure_cadence_excluded(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("exclude .cadence from imported repo git status");

    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import imported repo into repo-local db");

    let registry_path = app
        .state::<DesktopState>()
        .registry_file(&app.handle().clone())
        .expect("registry path");
    registry::replace_projects(
        &registry_path,
        vec![RegistryProjectRecord {
            project_id: repository.project_id.clone(),
            repository_id: repository.repository_id.clone(),
            root_path: root_path_string,
        }],
    )
    .expect("persist imported repo registry entry");

    (repository.project_id, canonical_root)
}

fn seed_authenticated_runtime(
    app: &tauri::App<tauri::test::MockRuntime>,
    auth_store_path: &Path,
    project_id: &str,
) {
    persist_openai_codex_session(
        auth_store_path,
        StoredOpenAiCodexSession {
            provider_id: "openai_codex".into(),
            session_id: "session-auth".into(),
            account_id: "acct-1".into(),
            access_token: "header.payload.signature".into(),
            refresh_token: "refresh-1".into(),
            expires_at: current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
            updated_at: "2026-04-18T19:00:00Z".into(),
        },
    )
    .expect("persist auth session");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("start runtime session");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
}

fn shell_argv(script: impl Into<String>) -> Vec<String> {
    let shell = runtime_shell::launch_script(script);
    std::iter::once(shell.program).chain(shell.args).collect()
}

fn wait_for_runtime_run(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    predicate: impl Fn(&RuntimeRunDto) -> bool,
) -> RuntimeRunDto {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let runtime_run = get_runtime_run(
            app.handle().clone(),
            app.state::<DesktopState>(),
            GetRuntimeRunRequestDto {
                project_id: project_id.into(),
            },
        )
        .expect("get runtime run should succeed")
        .expect("runtime run should exist");

        if predicate(&runtime_run) {
            return runtime_run;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for runtime run predicate, last snapshot: {runtime_run:?}"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

fn wait_for_autonomous_run(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    predicate: impl Fn(&AutonomousRunStateDto) -> bool,
) -> AutonomousRunStateDto {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let autonomous_run = get_autonomous_run(
            app.handle().clone(),
            app.state::<DesktopState>(),
            GetAutonomousRunRequestDto {
                project_id: project_id.into(),
            },
        )
        .expect("get autonomous run should succeed");

        if predicate(&autonomous_run) {
            return autonomous_run;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for autonomous run predicate, last snapshot: {autonomous_run:?}"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

fn load_git_statuses(repo_root: &Path) -> Vec<(String, Status)> {
    let repo = Repository::open(repo_root).expect("open imported git repo");
    let mut options = StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_unmodified(false);

    let statuses = repo
        .statuses(Some(&mut options))
        .expect("load git statuses");

    statuses
        .iter()
        .map(|entry| {
            (
                entry.path().expect("status path").to_string(),
                entry.status(),
            )
        })
        .collect()
}

fn stage_path(repo_root: &Path, relative_path: &str) {
    let repository = Repository::open(repo_root).expect("open imported git repo");
    let mut index = repository.index().expect("open imported git index");
    index
        .add_path(Path::new(relative_path))
        .expect("stage imported repo path");
    index.write().expect("write imported git index");
}

fn current_branch_name(repo_root: &Path) -> Option<String> {
    Repository::open(repo_root).ok().and_then(|repository| {
        repository
            .head()
            .ok()
            .and_then(|head| head.shorthand().map(ToOwned::to_owned))
    })
}

fn current_head_sha(repo_root: &Path) -> Option<String> {
    Repository::open(repo_root).ok().and_then(|repository| {
        repository
            .head()
            .ok()
            .and_then(|head| head.target().map(|oid| oid.to_string()))
    })
}

#[derive(Clone, Default)]
struct FixtureSkillSource {
    state: Arc<Mutex<FixtureSkillSourceState>>,
}

#[derive(Default)]
struct FixtureSkillSourceState {
    tree_response: Option<Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError>>,
    file_responses: BTreeMap<
        (String, String, String),
        Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError>,
    >,
    tree_requests: Vec<AutonomousSkillSourceTreeRequest>,
    file_requests: Vec<AutonomousSkillSourceFileRequest>,
}

impl FixtureSkillSource {
    fn set_tree_response(
        &self,
        response: Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError>,
    ) {
        self.state
            .lock()
            .expect("fixture source lock")
            .tree_response = Some(response);
    }

    fn set_file_text(&self, repo: &str, reference: &str, path: &str, content: &str) {
        self.state
            .lock()
            .expect("fixture source lock")
            .file_responses
            .insert(
                (repo.into(), reference.into(), path.into()),
                Ok(AutonomousSkillSourceFileResponse {
                    bytes: content.as_bytes().to_vec(),
                }),
            );
    }

    fn tree_request_count(&self) -> usize {
        self.state
            .lock()
            .expect("fixture source lock")
            .tree_requests
            .len()
    }

    fn file_request_count(&self) -> usize {
        self.state
            .lock()
            .expect("fixture source lock")
            .file_requests
            .len()
    }
}

impl AutonomousSkillSource for FixtureSkillSource {
    fn list_tree(
        &self,
        request: &AutonomousSkillSourceTreeRequest,
    ) -> Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError> {
        let mut state = self.state.lock().expect("fixture source lock");
        state.tree_requests.push(request.clone());
        state
            .tree_response
            .clone()
            .expect("fixture tree response should exist")
    }

    fn fetch_file(
        &self,
        request: &AutonomousSkillSourceFileRequest,
    ) -> Result<AutonomousSkillSourceFileResponse, AutonomousSkillSourceError> {
        let mut state = self.state.lock().expect("fixture source lock");
        state.file_requests.push(request.clone());
        state
            .file_responses
            .get(&(
                request.repo.clone(),
                request.reference.clone(),
                request.path.clone(),
            ))
            .cloned()
            .expect("fixture file response should exist")
    }
}

fn runtime_config() -> AutonomousSkillRuntimeConfig {
    AutonomousSkillRuntimeConfig {
        default_source_repo: "vercel-labs/skills".into(),
        default_source_ref: "main".into(),
        default_source_root: "skills".into(),
        github_api_base_url: "https://api.github.com".into(),
        github_token: None,
        limits: Default::default(),
    }
}

fn skill_source_metadata(skill_id: &str, tree_hash: &str) -> AutonomousSkillSourceMetadata {
    AutonomousSkillSourceMetadata {
        repo: "vercel-labs/skills".into(),
        path: format!("skills/{skill_id}"),
        reference: "main".into(),
        tree_hash: tree_hash.into(),
    }
}

fn standard_skill_tree(skill_id: &str, tree_hash: &str) -> AutonomousSkillSourceTreeResponse {
    AutonomousSkillSourceTreeResponse {
        entries: vec![
            AutonomousSkillSourceTreeEntry {
                path: format!("skills/{skill_id}"),
                kind: AutonomousSkillSourceEntryKind::Tree,
                hash: tree_hash.into(),
                bytes: None,
            },
            AutonomousSkillSourceTreeEntry {
                path: format!("skills/{skill_id}/SKILL.md"),
                kind: AutonomousSkillSourceEntryKind::Blob,
                hash: "1111111111111111111111111111111111111111".into(),
                bytes: Some(256),
            },
            AutonomousSkillSourceTreeEntry {
                path: format!("skills/{skill_id}/guide.md"),
                kind: AutonomousSkillSourceEntryKind::Blob,
                hash: "2222222222222222222222222222222222222222".into(),
                bytes: Some(64),
            },
        ],
    }
}

fn read_manifest(cache_root: &Path, cache_key: &str) -> AutonomousSkillCacheManifest {
    let manifest_path = cache_root.join(cache_key).join("manifest.json");
    let contents = fs::read_to_string(&manifest_path).expect("read manifest file");
    serde_json::from_str(&contents).expect("decode manifest")
}

#[test]
fn imported_repo_bridge_executes_repo_scoped_tool_operations_and_surfaces_git_changes() {
    let _guard = supervisor_test_lock::lock_supervisor_test_process();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for imported repo");

    let read = runtime
        .read(AutonomousReadRequest {
            path: "README.md".into(),
            start_line: Some(1),
            line_count: Some(2),
        })
        .expect("read imported repo file");
    match read.output {
        AutonomousToolOutput::Read(output) => {
            assert_eq!(output.path, "README.md");
            assert_eq!(output.content, "alpha\nbeta\n");
            assert_eq!(output.total_lines, 2);
        }
        other => panic!("unexpected read output: {other:?}"),
    }

    let written = runtime
        .write(AutonomousWriteRequest {
            path: "notes/proof.txt".into(),
            content: "bridge proof\n".into(),
        })
        .expect("write imported repo file");
    match written.output {
        AutonomousToolOutput::Write(output) => {
            assert_eq!(output.path, "notes/proof.txt");
            assert!(output.created);
        }
        other => panic!("unexpected write output: {other:?}"),
    }

    let find = runtime
        .find(AutonomousFindRequest {
            pattern: "*.txt".into(),
            path: Some("notes".into()),
        })
        .expect("find imported repo files inside notes");
    match find.output {
        AutonomousToolOutput::Find(output) => {
            assert_eq!(output.scope.as_deref(), Some("notes"));
            assert_eq!(output.matches, vec!["notes/proof.txt"]);
            assert!(!output.truncated);
        }
        other => panic!("unexpected find output: {other:?}"),
    }
    stage_path(&repo_root, "notes/proof.txt");

    let edited = runtime
        .edit(AutonomousEditRequest {
            path: "README.md".into(),
            start_line: 2,
            end_line: 2,
            expected: "beta\n".into(),
            replacement: "delta\n".into(),
        })
        .expect("edit imported repo readme");
    match edited.output {
        AutonomousToolOutput::Edit(output) => {
            assert_eq!(output.path, "README.md");
            assert_eq!(output.start_line, 2);
            assert_eq!(output.end_line, 2);
        }
        other => panic!("unexpected edit output: {other:?}"),
    }

    let git_status = runtime
        .git_status(AutonomousGitStatusRequest::default())
        .expect("imported repo git status succeeds");
    match git_status.output {
        AutonomousToolOutput::GitStatus(output) => {
            assert_eq!(output.changed_files, 2);
            assert_eq!(
                output.branch.as_ref().map(|branch| branch.name.clone()),
                current_branch_name(&repo_root)
            );
            assert!(output.has_staged_changes);
            assert!(output.has_unstaged_changes);
            assert!(!output.has_untracked_changes);
            assert!(output.entries.iter().any(|entry| {
                entry.path == "README.md"
                    && entry.unstaged == Some(cadence_desktop_lib::commands::ChangeKind::Modified)
            }));
            assert!(output.entries.iter().any(|entry| {
                entry.path == "notes/proof.txt"
                    && entry.staged == Some(cadence_desktop_lib::commands::ChangeKind::Added)
            }));
        }
        other => panic!("unexpected git status output: {other:?}"),
    }

    let staged_diff = runtime
        .git_diff(AutonomousGitDiffRequest {
            scope: RepositoryDiffScope::Staged,
        })
        .expect("imported repo staged diff succeeds");
    match staged_diff.output {
        AutonomousToolOutput::GitDiff(output) => {
            assert_eq!(output.scope, RepositoryDiffScope::Staged);
            assert_eq!(output.changed_files, 1);
            assert_eq!(output.base_revision, current_head_sha(&repo_root));
            assert!(output.patch.contains("notes/proof.txt"));
            assert!(!output.patch.contains("README.md"));
        }
        other => panic!("unexpected staged git diff output: {other:?}"),
    }

    let unstaged_diff = runtime
        .git_diff(AutonomousGitDiffRequest {
            scope: RepositoryDiffScope::Unstaged,
        })
        .expect("imported repo unstaged diff succeeds");
    match unstaged_diff.output {
        AutonomousToolOutput::GitDiff(output) => {
            assert_eq!(output.scope, RepositoryDiffScope::Unstaged);
            assert_eq!(output.changed_files, 1);
            assert_eq!(output.base_revision, None);
            assert!(output.patch.contains("README.md"));
            assert!(!output.patch.contains("notes/proof.txt"));
        }
        other => panic!("unexpected unstaged git diff output: {other:?}"),
    }

    let worktree_diff = runtime
        .git_diff(AutonomousGitDiffRequest {
            scope: RepositoryDiffScope::Worktree,
        })
        .expect("imported repo worktree diff succeeds");
    match worktree_diff.output {
        AutonomousToolOutput::GitDiff(output) => {
            assert_eq!(output.scope, RepositoryDiffScope::Worktree);
            assert_eq!(output.changed_files, 2);
            assert_eq!(output.base_revision, current_head_sha(&repo_root));
            assert!(output.patch.contains("README.md"));
            assert!(output.patch.contains("notes/proof.txt"));
        }
        other => panic!("unexpected worktree git diff output: {other:?}"),
    }

    let command = runtime
        .command(AutonomousCommandRequest {
            argv: shell_argv(if cfg!(windows) { "cd" } else { "pwd" }),
            cwd: Some("notes".into()),
            timeout_ms: Some(2_000),
        })
        .expect("run repo-scoped command");
    match command.output {
        AutonomousToolOutput::Command(output) => {
            assert_eq!(output.cwd, "notes");
            assert_eq!(output.exit_code, Some(0));
            let stdout = output.stdout.expect("command stdout should be captured");
            assert!(
                stdout.contains("notes"),
                "expected repo-scoped cwd in stdout: {stdout}"
            );
        }
        other => panic!("unexpected command output: {other:?}"),
    }

    assert_eq!(
        fs::read_to_string(repo_root.join("README.md")).expect("read edited readme"),
        "alpha\ndelta\n"
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("notes").join("proof.txt"))
            .expect("read written proof file"),
        "bridge proof\n"
    );

    let statuses = load_git_statuses(&repo_root);
    let readme_status = statuses
        .iter()
        .find(|(path, _)| path == "README.md")
        .map(|(_, status)| *status)
        .expect("README.md should be present in git status");
    let proof_status = statuses
        .iter()
        .find(|(path, _)| path == "notes/proof.txt")
        .map(|(_, status)| *status)
        .expect("notes/proof.txt should be present in git status");

    assert!(
        readme_status.contains(Status::WT_MODIFIED)
            || readme_status.contains(Status::INDEX_MODIFIED),
        "expected README.md to show a modified status, got {readme_status:?}"
    );
    assert!(
        proof_status.contains(Status::WT_NEW) || proof_status.contains(Status::INDEX_NEW),
        "expected notes/proof.txt to show a new-file status, got {proof_status:?}"
    );
}

#[test]
fn imported_repo_bridge_start_once_survives_reload_without_duplicate_continuation() {
    let _guard = supervisor_test_lock::lock_supervisor_test_process();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let started = start_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start autonomous run on imported repo");
    let started_run = started
        .run
        .expect("autonomous start should return run state");
    assert!(!started_run.duplicate_start_detected);
    assert!(matches!(
        started_run.status,
        AutonomousRunStatusDto::Starting | AutonomousRunStatusDto::Running
    ));

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == started_run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });

    let running = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        let Some(unit) = autonomous_state.unit.as_ref() else {
            return false;
        };

        run.run_id == started_run.run_id
            && matches!(
                run.status,
                AutonomousRunStatusDto::Starting | AutonomousRunStatusDto::Running
            )
            && run.recovery_state == AutonomousRunRecoveryStateDto::Healthy
            && run.active_unit_id.as_deref() == Some(unit.unit_id.as_str())
    });
    let running_run = running
        .run
        .as_ref()
        .expect("running autonomous run should exist");
    let running_unit = running
        .unit
        .as_ref()
        .expect("running autonomous unit should exist");
    assert_eq!(running_run.run_id, started_run.run_id);
    assert_eq!(
        running_run.active_unit_id.as_deref(),
        Some(running_unit.unit_id.as_str())
    );

    let duplicate = start_autonomous_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartAutonomousRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("duplicate autonomous start should reconnect");
    let duplicate_run = duplicate
        .run
        .expect("duplicate autonomous start should return run state");
    assert_eq!(duplicate_run.run_id, started_run.run_id);
    assert!(duplicate_run.duplicate_start_detected);
    assert_eq!(
        duplicate_run.duplicate_start_run_id.as_deref(),
        Some(started_run.run_id.as_str())
    );
    assert_eq!(
        duplicate_run.duplicate_start_reason.as_deref(),
        Some(
            "Cadence reused the already-active autonomous run for this project instead of launching a duplicate supervisor."
        )
    );

    let (fresh_state, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let recovered = wait_for_autonomous_run(&fresh_app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        let Some(unit) = autonomous_state.unit.as_ref() else {
            return false;
        };

        run.run_id == started_run.run_id
            && matches!(
                run.status,
                AutonomousRunStatusDto::Starting | AutonomousRunStatusDto::Running
            )
            && run.recovery_state == AutonomousRunRecoveryStateDto::Healthy
            && run.active_unit_id.as_deref() == Some(unit.unit_id.as_str())
    });
    let recovered_run = recovered
        .run
        .as_ref()
        .expect("recovered autonomous run should exist");
    let recovered_unit = recovered
        .unit
        .as_ref()
        .expect("recovered autonomous unit should exist");

    assert_eq!(recovered_run.run_id, started_run.run_id);
    assert_eq!(
        recovered_run.active_unit_id.as_deref(),
        Some(recovered_unit.unit_id.as_str())
    );

    let cancelled = cancel_autonomous_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        CancelAutonomousRunRequestDto {
            project_id: project_id.clone(),
            run_id: started_run.run_id.clone(),
        },
    )
    .expect("cancel imported repo autonomous run after reload")
    .run
    .expect("cancelled imported repo autonomous run should still exist");
    assert_eq!(cancelled.status, AutonomousRunStatusDto::Cancelled);
    assert_eq!(
        cancelled.recovery_state,
        AutonomousRunRecoveryStateDto::Terminal
    );

    let statuses = load_git_statuses(&repo_root);
    assert!(statuses.is_empty(), "shell-only duplicate-start proof should not mutate the imported repo worktree, got {statuses:?}");
}

#[test]
fn imported_repo_skill_runtime_uses_Cadence_cache_boundary_and_keeps_repo_clean() {
    let _guard = supervisor_test_lock::lock_supervisor_test_process();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let cache_root = app
        .state::<DesktopState>()
        .autonomous_skill_cache_dir(&app.handle().clone())
        .expect("autonomous skill cache dir");

    let source = FixtureSkillSource::default();
    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Imported repo proof.\nuser-invocable: false\n---\n\n# Find Skills\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "first guide\n",
    );

    let runtime = AutonomousSkillRuntime::with_source_and_cache(
        runtime_config(),
        Arc::new(source.clone()),
        Arc::new(FilesystemAutonomousSkillCacheStore::new(cache_root.clone())),
    );

    let initial_source =
        skill_source_metadata("find-skills", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    let first_install = runtime
        .install(
            cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
                source: initial_source.clone(),
                timeout_ms: Some(1_000),
            },
        )
        .expect("initial imported-repo skill install should succeed");
    let cached_invoke = runtime
        .invoke(cadence_desktop_lib::runtime::AutonomousSkillInvokeRequest {
            source: initial_source,
            timeout_ms: Some(1_000),
        })
        .expect("cached imported-repo skill invoke should succeed");

    source.set_tree_response(Ok(standard_skill_tree(
        "find-skills",
        "cccccccccccccccccccccccccccccccccccccccc",
    )));
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/SKILL.md",
        "---\nname: find-skills\ndescription: Imported repo proof refreshed.\nuser-invocable: false\n---\n\n# Find Skills\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "second guide\n",
    );

    let refreshed = runtime
        .install(
            cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
                source: skill_source_metadata(
                    "find-skills",
                    "cccccccccccccccccccccccccccccccccccccccc",
                ),
                timeout_ms: Some(1_000),
            },
        )
        .expect("refreshed imported-repo skill install should succeed");

    assert_eq!(project_id, "project-1");
    assert_eq!(first_install.cache_status, AutonomousSkillCacheStatus::Miss);
    assert_eq!(cached_invoke.cache_status, AutonomousSkillCacheStatus::Hit);
    assert_eq!(
        refreshed.cache_status,
        AutonomousSkillCacheStatus::Refreshed
    );
    assert_eq!(first_install.cache_key, refreshed.cache_key);
    assert!(
        Path::new(&first_install.cache_directory).starts_with(&cache_root),
        "expected Cadence to install imported-repo skills under app data, got {}",
        first_install.cache_directory
    );
    assert!(
        Path::new(&refreshed.cache_directory).starts_with(&cache_root),
        "expected refreshed imported-repo skills under app data, got {}",
        refreshed.cache_directory
    );
    assert!(!Path::new(&first_install.cache_directory).starts_with(&repo_root));
    assert!(!Path::new(&refreshed.cache_directory).starts_with(&repo_root));

    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let global_agents = home.join(".agents").join("skills");
        let global_pi = home.join(".pi").join("agent").join("skills");
        assert!(
            !Path::new(&first_install.cache_directory).starts_with(&global_agents),
            "Cadence must never install imported-repo skills into ~/.agents/skills"
        );
        assert!(
            !Path::new(&first_install.cache_directory).starts_with(&global_pi),
            "Cadence must never install imported-repo skills into ~/.pi/agent/skills"
        );
        assert!(
            !Path::new(&refreshed.cache_directory).starts_with(&global_agents),
            "Cadence must never refresh imported-repo skills inside ~/.agents/skills"
        );
        assert!(
            !Path::new(&refreshed.cache_directory).starts_with(&global_pi),
            "Cadence must never refresh imported-repo skills inside ~/.pi/agent/skills"
        );
    }

    let manifest = read_manifest(&cache_root, &refreshed.cache_key);
    assert_eq!(manifest.skill_id, "find-skills");
    assert_eq!(manifest.description, "Imported repo proof refreshed.");
    assert_eq!(
        manifest.source.tree_hash,
        "cccccccccccccccccccccccccccccccccccccccc"
    );
    assert!(
        cache_root
            .join(&refreshed.cache_key)
            .join("trees")
            .join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .is_dir(),
        "expected the initial tree revision to remain addressable inside the Cadence cache"
    );
    assert!(
        cache_root
            .join(&refreshed.cache_key)
            .join("trees")
            .join("cccccccccccccccccccccccccccccccccccccccc")
            .is_dir(),
        "expected the refreshed tree revision to be written under the same Cadence cache key"
    );
    assert_eq!(source.tree_request_count(), 2);
    assert_eq!(source.file_request_count(), 4);

    let statuses = load_git_statuses(&repo_root);
    assert!(
        statuses.is_empty(),
        "skill runtime cache activity must not dirty the imported repo worktree, got {statuses:?}"
    );
    assert!(!repo_root.join(".agents").exists());
    assert!(!repo_root.join(".pi").exists());
    assert!(!repo_root
        .join(".cadence")
        .join("autonomous-skills")
        .exists());
}
