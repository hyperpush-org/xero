use std::{
    fs,
    path::{Path, PathBuf},
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
        GetAutonomousRunRequestDto, GetRuntimeRunRequestDto, ProjectIdRequestDto, RuntimeAuthPhase,
        RuntimeRunDto, RuntimeRunStatusDto, RuntimeRunTransportLivenessDto,
        StartAutonomousRunRequestDto,
    },
    configure_builder_with_state, db,
    git::repository::{ensure_cadence_excluded, CanonicalRepository},
    registry::{self, RegistryProjectRecord},
    runtime::{
        AutonomousCommandRequest, AutonomousEditRequest, AutonomousReadRequest,
        AutonomousToolOutput, AutonomousToolRuntime, AutonomousWriteRequest,
    },
    state::DesktopState,
};
use git2::{Repository, Status, StatusOptions};
use tauri::Manager;
use tempfile::TempDir;

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

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
            .with_runtime_supervisor_binary_override(supervisor_binary_path()),
        auth_store_path,
    )
}

fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cadence-runtime-supervisor"))
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
        git2::Signature::now("Cadence Test", "cadence@example.com").expect("create test signature");

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

#[test]
fn imported_repo_bridge_executes_repo_scoped_tool_operations_and_surfaces_git_changes() {
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
