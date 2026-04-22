use std::{
    fs,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    thread,
};

use cadence_desktop_lib::{
    commands::{
        RepositoryDiffScope, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlStateDto, RuntimeRunPendingControlSnapshotDto,
    },
    configure_builder_with_state, db,
    git::{
        diff::MAX_PATCH_BYTES,
        repository::{ensure_cadence_excluded, CanonicalRepository},
    },
    registry::{self, RegistryProjectRecord},
    runtime::{
        AutonomousCommandPolicyOutcome, AutonomousCommandRequest, AutonomousEditRequest,
        AutonomousFindRequest, AutonomousGitDiffRequest, AutonomousGitStatusRequest,
        AutonomousReadRequest, AutonomousSearchRequest, AutonomousToolOutput,
        AutonomousToolRequest, AutonomousToolRuntime, AutonomousToolRuntimeLimits,
        AutonomousWebConfig, AutonomousWebFetchContentKind, AutonomousWebFetchRequest,
        AutonomousWebSearchProviderConfig, AutonomousWebSearchRequest, AutonomousWriteRequest,
    },
    state::DesktopState,
};
use git2::{IndexAddOption, Repository, Signature};
use tauri::Manager;
use tempfile::TempDir;

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    DesktopState::default()
        .with_registry_file_override(root.path().join("app-data").join("project-registry.json"))
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> (String, PathBuf) {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(repo_root.join("src")).expect("create repo src");
    fs::write(repo_root.join("src").join("tracked.txt"), "alpha\n").expect("seed tracked file");

    let git_repository = Repository::init(&repo_root).expect("init git repo");
    commit_all(&git_repository, "initial commit");

    let canonical_root = fs::canonicalize(&repo_root).expect("canonical repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();

    let repository = CanonicalRepository {
        project_id: "project-1".into(),
        repository_id: "repo-1".into(),
        root_path: canonical_root.clone(),
        root_path_string: root_path_string.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "repo".into(),
        branch_name: current_branch_name(&canonical_root),
        head_sha: current_head_sha(&canonical_root),
        branch: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

    ensure_cadence_excluded(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("exclude .cadence from seeded repo git status");

    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import project into repo-local db");

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
    .expect("persist registry entry");

    (repository.project_id, canonical_root)
}

fn commit_all(repository: &Repository, message: &str) {
    let mut index = repository.index().expect("repo index");
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .expect("stage files");
    index.write().expect("write index");

    let tree_id = index.write_tree().expect("write tree");
    let tree = repository.find_tree(tree_id).expect("find tree");
    let signature = Signature::now("Cadence", "Cadence@example.com").expect("signature");

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

fn stage_path(repo_root: &Path, relative_path: &str) {
    let repository = Repository::open(repo_root).expect("open git repo");
    let mut index = repository.index().expect("repo index");
    index
        .add_path(Path::new(relative_path))
        .expect("stage path");
    index.write().expect("write index");
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

fn runtime_control_state(
    active: RuntimeRunApprovalModeDto,
    pending: Option<RuntimeRunApprovalModeDto>,
) -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            model_id: "model-1".into(),
            thinking_effort: None,
            approval_mode: active,
            revision: 1,
            applied_at: "2026-04-22T00:00:00Z".into(),
        },
        pending: pending.map(|approval_mode| RuntimeRunPendingControlSnapshotDto {
            model_id: "model-1".into(),
            thinking_effort: None,
            approval_mode,
            revision: 2,
            queued_at: "2026-04-22T00:01:00Z".into(),
            queued_prompt: None,
            queued_prompt_at: None,
        }),
    }
}

fn runtime_for_project_with_controls(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    controls: RuntimeRunControlStateDto,
) -> AutonomousToolRuntime {
    AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        project_id,
    )
    .expect("build autonomous tool runtime")
    .with_runtime_run_controls(controls)
}

fn runtime_for_project_with_approval(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    active: RuntimeRunApprovalModeDto,
) -> AutonomousToolRuntime {
    runtime_for_project_with_controls(app, project_id, runtime_control_state(active, None))
}

fn shell_argv(script: impl Into<String>) -> Vec<String> {
    let shell = runtime_shell::launch_script(script);
    std::iter::once(shell.program).chain(shell.args).collect()
}

fn spawn_static_http_server(status: u16, content_type: &str, body: &str) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test http server");
    let address = listener.local_addr().expect("test http server addr");
    let content_type = content_type.to_string();
    let body = body.to_string();

    thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept test http request");
        let mut reader = BufReader::new(stream.try_clone().expect("clone tcp stream"));
        let mut line = String::new();
        loop {
            line.clear();
            let bytes = reader.read_line(&mut line).expect("read request line");
            if bytes == 0 || line == "\r\n" {
                break;
            }
        }

        write!(
            stream,
            "HTTP/1.1 {status} Test\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body,
        )
        .expect("write test http response");
    });

    format!("http://{address}")
}

#[test]
fn tool_runtime_executes_repo_scoped_operations_and_returns_stable_envelopes() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    fs::write(
        repo_root.join("src").join("app.txt"),
        "alpha\nbeta\ngamma\n",
    )
    .expect("seed repo file");
    fs::create_dir_all(repo_root.join("src").join("nested")).expect("create nested repo dir");
    fs::write(
        repo_root.join("src").join("nested").join("inner.txt"),
        "nested\n",
    )
    .expect("seed nested repo file");
    fs::create_dir_all(repo_root.join("node_modules").join("pkg"))
        .expect("create skipped node_modules dir");
    fs::write(
        repo_root
            .join("node_modules")
            .join("pkg")
            .join("ignored.txt"),
        "beta\n",
    )
    .expect("seed skipped dependency file");

    let runtime =
        runtime_for_project_with_approval(&app, &project_id, RuntimeRunApprovalModeDto::Yolo);

    let read = runtime
        .read(AutonomousReadRequest {
            path: "src/app.txt".into(),
            start_line: Some(2),
            line_count: Some(2),
        })
        .expect("read file inside repo");
    assert_eq!(read.tool_name, "read");
    match read.output {
        AutonomousToolOutput::Read(output) => {
            assert_eq!(output.path, "src/app.txt");
            assert_eq!(output.start_line, 2);
            assert_eq!(output.line_count, 2);
            assert_eq!(output.total_lines, 3);
            assert_eq!(output.content, "beta\ngamma\n");
            assert!(!output.truncated);
        }
        other => panic!("unexpected read output: {other:?}"),
    }

    let search = runtime
        .search(AutonomousSearchRequest {
            query: "beta".into(),
            path: Some("src".into()),
        })
        .expect("search repo text");
    assert_eq!(search.tool_name, "search");
    match search.output {
        AutonomousToolOutput::Search(output) => {
            assert_eq!(output.scope.as_deref(), Some("src"));
            assert_eq!(output.matches.len(), 1);
            assert_eq!(output.matches[0].path, "src/app.txt");
            assert_eq!(output.matches[0].line, 2);
            assert_eq!(output.matches[0].column, 1);
            assert_eq!(output.scanned_files, 3);
        }
        other => panic!("unexpected search output: {other:?}"),
    }

    let find = runtime
        .find(AutonomousFindRequest {
            pattern: "**/*.txt".into(),
            path: Some("src".into()),
        })
        .expect("find repo files");
    assert_eq!(find.tool_name, "find");
    match find.output {
        AutonomousToolOutput::Find(output) => {
            assert_eq!(output.pattern, "**/*.txt");
            assert_eq!(output.scope.as_deref(), Some("src"));
            assert_eq!(
                output.matches,
                vec!["src/app.txt", "src/nested/inner.txt", "src/tracked.txt"]
            );
            assert_eq!(output.scanned_files, 3);
            assert!(!output.truncated);
        }
        other => panic!("unexpected find output: {other:?}"),
    }

    let written = runtime
        .write(AutonomousWriteRequest {
            path: "notes/output.txt".into(),
            content: "hello from Cadence\n".into(),
        })
        .expect("write file inside repo");
    assert_eq!(written.tool_name, "write");
    match written.output {
        AutonomousToolOutput::Write(output) => {
            assert_eq!(output.path, "notes/output.txt");
            assert!(output.created);
            assert_eq!(
                fs::read_to_string(repo_root.join("notes").join("output.txt"))
                    .expect("read written file"),
                "hello from Cadence\n"
            );
        }
        other => panic!("unexpected write output: {other:?}"),
    }

    let edited = runtime
        .edit(AutonomousEditRequest {
            path: "src/app.txt".into(),
            start_line: 2,
            end_line: 2,
            expected: "beta\n".into(),
            replacement: "delta\n".into(),
        })
        .expect("edit file inside repo");
    assert_eq!(edited.tool_name, "edit");
    match edited.output {
        AutonomousToolOutput::Edit(output) => {
            assert_eq!(output.path, "src/app.txt");
            assert_eq!(output.start_line, 2);
            assert_eq!(output.end_line, 2);
            assert_eq!(
                fs::read_to_string(repo_root.join("src").join("app.txt"))
                    .expect("read edited file"),
                "alpha\ndelta\ngamma\n"
            );
        }
        other => panic!("unexpected edit output: {other:?}"),
    }

    let command = runtime
        .command(AutonomousCommandRequest {
            argv: vec!["git".into(), "rev-parse".into(), "--show-prefix".into()],
            cwd: Some("notes".into()),
            timeout_ms: Some(2_000),
        })
        .expect("run repo-scoped command");
    assert_eq!(command.tool_name, "command");
    assert_eq!(
        command
            .command_result
            .as_ref()
            .and_then(|result| result.exit_code),
        Some(0)
    );
    assert_eq!(
        command
            .command_result
            .as_ref()
            .map(|result| result.policy.outcome.clone()),
        Some(AutonomousCommandPolicyOutcome::Allowed)
    );
    match command.output {
        AutonomousToolOutput::Command(output) => {
            assert_eq!(output.cwd, "notes");
            assert_eq!(output.exit_code, Some(0));
            assert!(output.spawned);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Allowed
            );
            let stdout = output.stdout.expect("stdout captured");
            assert_eq!(stdout, "notes/");
        }
        other => panic!("unexpected command output: {other:?}"),
    }
}

#[test]
fn tool_runtime_executes_web_search_and_fetch_with_backend_owned_config() {
    let search_base_url = spawn_static_http_server(
        200,
        "application/json",
        &serde_json::json!({
            "results": [
                {
                    "title": "Rust result",
                    "url": "https://example.com/rust",
                    "snippet": "Rust &amp; systems"
                },
                {
                    "title": "Second result",
                    "url": "https://example.com/second",
                    "snippet": null
                }
            ]
        })
        .to_string(),
    );
    let fetch_base_url = spawn_static_http_server(
        200,
        "text/html; charset=utf-8",
        "<!doctype html><html><head><title>Example Page</title></head><body><h1>Heading</h1><p>Alpha &amp; beta</p></body></html>",
    );

    let root = tempfile::tempdir().expect("temp dir");
    let state = create_state(&root).with_autonomous_web_config_override(AutonomousWebConfig {
        search_provider: Some(AutonomousWebSearchProviderConfig::new(format!(
            "{search_base_url}/search"
        ))),
        limits: Default::default(),
    });
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let search = runtime
        .web_search(AutonomousWebSearchRequest {
            query: "rust".into(),
            result_count: Some(1),
            timeout_ms: Some(1_000),
        })
        .expect("web search should succeed");
    assert_eq!(search.tool_name, "web_search");
    match search.output {
        AutonomousToolOutput::WebSearch(output) => {
            assert_eq!(output.results.len(), 1);
            assert_eq!(output.results[0].title, "Rust result");
            assert_eq!(output.results[0].snippet.as_deref(), Some("Rust & systems"));
            assert!(output.truncated);
        }
        other => panic!("unexpected web search output: {other:?}"),
    }

    let fetch = runtime
        .web_fetch(AutonomousWebFetchRequest {
            url: format!("{fetch_base_url}/page"),
            max_chars: Some(200),
            timeout_ms: Some(1_000),
        })
        .expect("web fetch should succeed");
    assert_eq!(fetch.tool_name, "web_fetch");
    match fetch.output {
        AutonomousToolOutput::WebFetch(output) => {
            assert_eq!(output.content_type.as_deref(), Some("text/html"));
            assert_eq!(output.content_kind, AutonomousWebFetchContentKind::Html);
            assert_eq!(output.title.as_deref(), Some("Example Page"));
            assert!(output.content.contains("Heading"));
            assert!(output.content.contains("Alpha & beta"));
            assert!(!output.truncated);
        }
        other => panic!("unexpected web fetch output: {other:?}"),
    }

    let provider_missing = AutonomousToolRuntime::with_limits_and_web_config(
        root.path().join("repo"),
        AutonomousToolRuntimeLimits::default(),
        AutonomousWebConfig::default(),
    )
    .expect("build runtime without backend web config");
    let provider_missing_error = provider_missing
        .web_search(AutonomousWebSearchRequest {
            query: "rust".into(),
            result_count: None,
            timeout_ms: None,
        })
        .expect_err("missing backend provider config should fail closed");
    assert_eq!(
        provider_missing_error.code,
        "autonomous_web_search_provider_unavailable"
    );

    let invalid_fetch_error = runtime
        .web_fetch(AutonomousWebFetchRequest {
            url: "mailto:test@example.com".into(),
            max_chars: None,
            timeout_ms: None,
        })
        .expect_err("unsupported fetch schemes should fail closed");
    assert_eq!(
        invalid_fetch_error.code,
        "autonomous_web_fetch_scheme_unsupported"
    );
}

#[test]
fn tool_runtime_executes_git_status_and_diff_with_real_repository_truth() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    fs::write(repo_root.join("src").join("tracked.txt"), "alpha\nbeta\n")
        .expect("modify tracked file");
    fs::write(repo_root.join("src").join("staged.txt"), "staged change\n")
        .expect("write staged file");
    stage_path(&repo_root, "src/staged.txt");

    let status = runtime
        .git_status(AutonomousGitStatusRequest::default())
        .expect("git status succeeds");
    assert_eq!(status.tool_name, "git_status");
    match status.output {
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
                entry.path == "src/tracked.txt"
                    && entry.unstaged == Some(cadence_desktop_lib::commands::ChangeKind::Modified)
            }));
            assert!(output.entries.iter().any(|entry| {
                entry.path == "src/staged.txt"
                    && entry.staged == Some(cadence_desktop_lib::commands::ChangeKind::Added)
            }));
        }
        other => panic!("unexpected git status output: {other:?}"),
    }

    let staged_diff = runtime
        .git_diff(AutonomousGitDiffRequest {
            scope: RepositoryDiffScope::Staged,
        })
        .expect("staged diff succeeds");
    match staged_diff.output {
        AutonomousToolOutput::GitDiff(output) => {
            assert_eq!(output.scope, RepositoryDiffScope::Staged);
            assert_eq!(output.changed_files, 1);
            assert_eq!(
                output.branch.as_ref().map(|branch| branch.name.clone()),
                current_branch_name(&repo_root)
            );
            assert_eq!(output.base_revision, current_head_sha(&repo_root));
            assert!(!output.truncated);
            assert!(output.patch.contains("staged.txt"));
            assert!(!output.patch.contains("tracked.txt"));
        }
        other => panic!("unexpected staged diff output: {other:?}"),
    }

    let unstaged_diff = runtime
        .git_diff(AutonomousGitDiffRequest {
            scope: RepositoryDiffScope::Unstaged,
        })
        .expect("unstaged diff succeeds");
    match unstaged_diff.output {
        AutonomousToolOutput::GitDiff(output) => {
            assert_eq!(output.scope, RepositoryDiffScope::Unstaged);
            assert_eq!(output.changed_files, 1);
            assert_eq!(output.base_revision, None);
            assert!(!output.truncated);
            assert!(output.patch.contains("tracked.txt"));
            assert!(!output.patch.contains("staged.txt"));
        }
        other => panic!("unexpected unstaged diff output: {other:?}"),
    }

    let worktree_diff = runtime
        .git_diff(AutonomousGitDiffRequest {
            scope: RepositoryDiffScope::Worktree,
        })
        .expect("worktree diff succeeds");
    match worktree_diff.output {
        AutonomousToolOutput::GitDiff(output) => {
            assert_eq!(output.scope, RepositoryDiffScope::Worktree);
            assert_eq!(output.changed_files, 2);
            assert_eq!(output.base_revision, current_head_sha(&repo_root));
            assert!(!output.truncated);
            assert!(output.patch.contains("tracked.txt"));
            assert!(output.patch.contains("staged.txt"));
        }
        other => panic!("unexpected worktree diff output: {other:?}"),
    }

    fs::write(
        repo_root.join("src").join("untracked.txt"),
        "untracked change\n",
    )
    .expect("write untracked file");
    let status_with_untracked = runtime
        .git_status(AutonomousGitStatusRequest::default())
        .expect("git status with untracked file succeeds");
    match status_with_untracked.output {
        AutonomousToolOutput::GitStatus(output) => {
            assert_eq!(output.changed_files, 3);
            assert!(output.has_untracked_changes);
            assert!(output
                .entries
                .iter()
                .any(|entry| { entry.path == "src/untracked.txt" && entry.untracked }));
        }
        other => panic!("unexpected git status output with untracked file: {other:?}"),
    }
}

#[test]
fn tool_runtime_git_status_reports_detached_head_truthfully() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let repository = Repository::open(&repo_root).expect("open git repo");
    let head_oid = repository.head().expect("head").target().expect("head oid");
    let head_commit = repository.find_commit(head_oid).expect("find commit");
    repository
        .checkout_tree(head_commit.as_object(), None)
        .expect("checkout tree");
    repository.set_head_detached(head_oid).expect("detach head");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let status = runtime
        .git_status(AutonomousGitStatusRequest::default())
        .expect("git status succeeds on detached head");
    match status.output {
        AutonomousToolOutput::GitStatus(output) => {
            let branch = output.branch.expect("detached branch summary");
            assert!(branch.detached);
            assert_eq!(branch.name, "HEAD");
            assert_eq!(branch.head_sha, Some(head_oid.to_string()));
            assert_eq!(output.changed_files, 0);
        }
        other => panic!("unexpected git status output on detached head: {other:?}"),
    }
}

#[test]
fn tool_runtime_git_diff_reports_truncation_truthfully() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let large_patch =
        std::iter::repeat_n("line with plenty of diff payload\n", 5_000).collect::<String>();
    fs::write(repo_root.join("src").join("large.txt"), large_patch)
        .expect("write large patch fixture");
    stage_path(&repo_root, "src/large.txt");

    let diff = runtime
        .git_diff(AutonomousGitDiffRequest {
            scope: RepositoryDiffScope::Staged,
        })
        .expect("staged diff succeeds");
    match diff.output {
        AutonomousToolOutput::GitDiff(output) => {
            assert_eq!(output.changed_files, 1);
            assert!(output.truncated);
            assert!(output.patch.len() <= MAX_PATCH_BYTES);
            assert!(output.patch.contains("large.txt"));
        }
        other => panic!("unexpected truncated git diff output: {other:?}"),
    }
}

#[test]
fn tool_runtime_rejects_malformed_inputs_and_reports_error_paths_deterministically() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    fs::write(
        repo_root.join("src").join("app.txt"),
        "alpha\nbeta\ngamma\n",
    )
    .expect("seed repo file");
    fs::write(repo_root.join("binary.bin"), [0xff_u8, 0xfe, 0x00]).expect("seed binary file");
    fs::write(
        repo_root.join("large.txt"),
        "z".repeat(AutonomousToolRuntimeLimits::default().max_text_file_bytes + 1),
    )
    .expect("seed oversized text file");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let invalid_read = runtime
        .read(AutonomousReadRequest {
            path: "binary.bin".into(),
            start_line: None,
            line_count: None,
        })
        .expect_err("binary reads should be rejected");
    assert_eq!(invalid_read.code, "autonomous_tool_file_not_text");

    let oversized_query = "x".repeat(257);
    let search_error = runtime
        .search(AutonomousSearchRequest {
            query: oversized_query,
            path: None,
        })
        .expect_err("oversized search query should be rejected");
    assert_eq!(search_error.code, "autonomous_tool_search_query_too_large");

    let empty_search = runtime
        .search(AutonomousSearchRequest {
            query: "missing".into(),
            path: Some("src".into()),
        })
        .expect("zero-match search should still succeed");
    match empty_search.output {
        AutonomousToolOutput::Search(output) => assert!(output.matches.is_empty()),
        other => panic!("unexpected empty-search output: {other:?}"),
    }

    let search_with_binary_and_large_files = runtime
        .search(AutonomousSearchRequest {
            query: "gamma".into(),
            path: None,
        })
        .expect("search should skip binary and oversized files");
    match search_with_binary_and_large_files.output {
        AutonomousToolOutput::Search(output) => {
            assert_eq!(output.matches.len(), 1);
            assert_eq!(output.matches[0].path, "src/app.txt");
        }
        other => panic!("unexpected search output with skipped files: {other:?}"),
    }

    let empty_find = runtime
        .find(AutonomousFindRequest {
            pattern: "**/*.md".into(),
            path: Some("src".into()),
        })
        .expect("zero-match find should still succeed");
    match empty_find.output {
        AutonomousToolOutput::Find(output) => assert!(output.matches.is_empty()),
        other => panic!("unexpected empty-find output: {other:?}"),
    }

    let invalid_find_pattern = runtime
        .find(AutonomousFindRequest {
            pattern: "[*.txt".into(),
            path: None,
        })
        .expect_err("malformed find patterns should be rejected");
    assert_eq!(
        invalid_find_pattern.code,
        "autonomous_tool_find_pattern_invalid"
    );

    let invalid_find_scope = runtime
        .find(AutonomousFindRequest {
            pattern: "**/*.txt".into(),
            path: Some("../outside".into()),
        })
        .expect_err("find path traversal should be denied");
    assert_eq!(invalid_find_scope.code, "autonomous_tool_path_denied");

    let invalid_scope: Result<AutonomousToolRequest, _> =
        serde_json::from_value(serde_json::json!({
            "tool": "git_diff",
            "input": {
                "scope": "unsupported"
            }
        }));
    assert!(
        invalid_scope.is_err(),
        "unsupported autonomous git diff scope should fail request parsing"
    );

    let invalid_range = runtime
        .edit(AutonomousEditRequest {
            path: "src/app.txt".into(),
            start_line: 4,
            end_line: 5,
            expected: "placeholder\n".into(),
            replacement: "noop\n".into(),
        })
        .expect_err("out-of-range edit should be rejected");
    assert_eq!(invalid_range.code, "autonomous_tool_edit_range_invalid");

    runtime
        .edit(AutonomousEditRequest {
            path: "src/app.txt".into(),
            start_line: 2,
            end_line: 2,
            expected: "beta\n".into(),
            replacement: "delta\n".into(),
        })
        .expect("first deterministic edit succeeds");
    let deterministic_mismatch = runtime
        .edit(AutonomousEditRequest {
            path: "src/app.txt".into(),
            start_line: 2,
            end_line: 2,
            expected: "beta\n".into(),
            replacement: "delta\n".into(),
        })
        .expect_err("repeating stale edit should fail deterministically");
    assert_eq!(
        deterministic_mismatch.code,
        "autonomous_tool_edit_expected_text_mismatch"
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("src").join("app.txt")).expect("read edited file"),
        "alpha\ndelta\ngamma\n"
    );

    let yolo_runtime =
        runtime_for_project_with_approval(&app, &project_id, RuntimeRunApprovalModeDto::Yolo);

    let nonzero = yolo_runtime
        .command(AutonomousCommandRequest {
            argv: vec![
                "git".into(),
                "rev-parse".into(),
                "--verify".into(),
                "refs/heads/missing-branch".into(),
            ],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("non-zero exits should return a stable command result");
    assert_eq!(
        nonzero
            .command_result
            .as_ref()
            .and_then(|result| result.exit_code),
        Some(128)
    );
    match nonzero.output {
        AutonomousToolOutput::Command(output) => {
            assert_eq!(output.exit_code, Some(128));
            assert!(output.spawned);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Allowed
            );
            assert!(output.stderr.is_some());
        }
        other => panic!("unexpected non-zero command output: {other:?}"),
    }

    let timeout = yolo_runtime
        .command(AutonomousCommandRequest {
            argv: if cfg!(windows) {
                vec!["ping".into(), "-n".into(), "3".into(), "127.0.0.1".into()]
            } else {
                vec!["ping".into(), "-c".into(), "3".into(), "127.0.0.1".into()]
            },
            cwd: None,
            timeout_ms: Some(50),
        })
        .expect_err("timed-out command should return a retryable error");
    assert_eq!(timeout.code, "autonomous_tool_command_timeout");
    assert!(timeout.retryable);

    fs::remove_dir_all(repo_root.join(".git")).expect("remove git dir");

    let git_status_error = runtime
        .git_status(AutonomousGitStatusRequest::default())
        .expect_err("broken git state should fail git status");
    assert_eq!(git_status_error.code, "git_repository_not_found");

    let git_diff_error = runtime
        .git_diff(AutonomousGitDiffRequest {
            scope: RepositoryDiffScope::Worktree,
        })
        .expect_err("broken git state should fail git diff");
    assert_eq!(git_diff_error.code, "git_repository_not_found");
}

#[test]
fn tool_runtime_command_policy_uses_active_approval_snapshot_only() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let strict_runtime = runtime_for_project_with_controls(
        &app,
        &project_id,
        runtime_control_state(
            RuntimeRunApprovalModeDto::Suggest,
            Some(RuntimeRunApprovalModeDto::Yolo),
        ),
    );
    let strict = strict_runtime
        .command(AutonomousCommandRequest {
            argv: vec!["git".into(), "status".into(), "--short".into()],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("strict approval mode should return a review envelope");
    match strict.output {
        AutonomousToolOutput::Command(output) => {
            assert!(!output.spawned);
            assert_eq!(output.exit_code, None);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Escalated
            );
            assert_eq!(
                output.policy.approval_mode,
                RuntimeRunApprovalModeDto::Suggest
            );
            assert_eq!(output.policy.code, "policy_escalated_approval_mode");
        }
        other => panic!("unexpected strict command output: {other:?}"),
    }

    let yolo_runtime = runtime_for_project_with_controls(
        &app,
        &project_id,
        runtime_control_state(
            RuntimeRunApprovalModeDto::Yolo,
            Some(RuntimeRunApprovalModeDto::Suggest),
        ),
    );
    let allowed = yolo_runtime
        .command(AutonomousCommandRequest {
            argv: vec!["git".into(), "status".into(), "--short".into()],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("active yolo should allow safe git status");
    match allowed.output {
        AutonomousToolOutput::Command(output) => {
            assert!(output.spawned);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Allowed
            );
            assert_eq!(output.policy.approval_mode, RuntimeRunApprovalModeDto::Yolo);
            assert_eq!(output.policy.code, "policy_allowed_repo_scoped_command");
        }
        other => panic!("unexpected yolo command output: {other:?}"),
    }
}

#[test]
fn tool_runtime_command_policy_escalates_destructive_shell_wrappers_before_spawn() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime =
        runtime_for_project_with_approval(&app, &project_id, RuntimeRunApprovalModeDto::Yolo);
    let destructive = runtime
        .command(AutonomousCommandRequest {
            argv: shell_argv(if cfg!(windows) {
                "del /Q src\\tracked.txt"
            } else {
                "rm -rf src"
            }),
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("destructive shell wrapper should escalate before spawn");

    match destructive.output {
        AutonomousToolOutput::Command(output) => {
            assert!(!output.spawned);
            assert_eq!(output.exit_code, None);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Escalated
            );
            assert_eq!(output.policy.code, "policy_escalated_destructive_shell");
        }
        other => panic!("unexpected destructive shell output: {other:?}"),
    }
}

#[test]
fn tool_runtime_command_policy_fails_closed_for_ambiguous_commands() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime =
        runtime_for_project_with_approval(&app, &project_id, RuntimeRunApprovalModeDto::Yolo);
    let ambiguous = runtime
        .command(AutonomousCommandRequest {
            argv: vec!["madeup-command".into(), "--version".into()],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("ambiguous commands should fail closed before spawn");

    match ambiguous.output {
        AutonomousToolOutput::Command(output) => {
            assert!(!output.spawned);
            assert_eq!(output.exit_code, None);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Escalated
            );
            assert_eq!(output.policy.code, "policy_escalated_ambiguous_command");
        }
        other => panic!("unexpected ambiguous command output: {other:?}"),
    }
}

#[test]
fn tool_runtime_command_policy_denies_repo_escape_arguments_before_spawn() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime =
        runtime_for_project_with_approval(&app, &project_id, RuntimeRunApprovalModeDto::Yolo);
    let error = runtime
        .command(AutonomousCommandRequest {
            argv: vec![
                "git".into(),
                "diff".into(),
                "--".into(),
                "../outside.txt".into(),
            ],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect_err("repo escape arguments should be denied before spawn");

    assert_eq!(error.code, "policy_denied_argument_outside_repo");
    assert_eq!(
        error.class,
        cadence_desktop_lib::commands::CommandErrorClass::PolicyDenied
    );
}

#[test]
fn tool_runtime_reports_truncation_for_bounded_search_and_find_results() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (_project_id, repo_root) = seed_project(&root, &app);

    fs::create_dir_all(repo_root.join("fixtures")).expect("create fixture dir");
    for (file_name, contents) in [
        ("a.txt", "needle\n"),
        ("b.txt", "needle\n"),
        ("c.txt", "needle\n"),
    ] {
        fs::write(repo_root.join("fixtures").join(file_name), contents)
            .expect("seed truncation fixture");
    }

    let runtime = AutonomousToolRuntime::with_limits(
        &repo_root,
        AutonomousToolRuntimeLimits {
            max_search_results: 2,
            ..AutonomousToolRuntimeLimits::default()
        },
    )
    .expect("build bounded autonomous tool runtime");

    let search = runtime
        .search(AutonomousSearchRequest {
            query: "needle".into(),
            path: Some("fixtures".into()),
        })
        .expect("bounded search succeeds");
    match search.output {
        AutonomousToolOutput::Search(output) => {
            assert_eq!(output.matches.len(), 2);
            assert_eq!(
                output
                    .matches
                    .iter()
                    .map(|entry| entry.path.as_str())
                    .collect::<Vec<_>>(),
                vec!["fixtures/a.txt", "fixtures/b.txt"]
            );
            assert!(output.truncated);
        }
        other => panic!("unexpected bounded search output: {other:?}"),
    }

    let find = runtime
        .find(AutonomousFindRequest {
            pattern: "**/*.txt".into(),
            path: Some("fixtures".into()),
        })
        .expect("bounded find succeeds");
    match find.output {
        AutonomousToolOutput::Find(output) => {
            assert_eq!(output.matches, vec!["fixtures/a.txt", "fixtures/b.txt"]);
            assert!(output.truncated);
        }
        other => panic!("unexpected bounded find output: {other:?}"),
    }
}

#[test]
fn tool_runtime_denies_path_traversal_and_out_of_repo_cwds() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime =
        runtime_for_project_with_approval(&app, &project_id, RuntimeRunApprovalModeDto::Yolo);

    let read_error = runtime
        .read(AutonomousReadRequest {
            path: "../outside.txt".into(),
            start_line: None,
            line_count: None,
        })
        .expect_err("path traversal should be denied");
    assert_eq!(read_error.code, "autonomous_tool_path_denied");
    assert_eq!(
        read_error.class,
        cadence_desktop_lib::commands::CommandErrorClass::PolicyDenied
    );

    let write_error = runtime
        .write(AutonomousWriteRequest {
            path: "../outside.txt".into(),
            content: "denied".into(),
        })
        .expect_err("out-of-root write should be denied");
    assert_eq!(write_error.code, "autonomous_tool_path_denied");
    assert_eq!(
        write_error.class,
        cadence_desktop_lib::commands::CommandErrorClass::PolicyDenied
    );

    let cwd_error = runtime
        .command(AutonomousCommandRequest {
            argv: vec!["git".into(), "status".into(), "--short".into()],
            cwd: Some("../".into()),
            timeout_ms: Some(1_000),
        })
        .expect_err("out-of-root cwd should be denied");
    assert_eq!(cwd_error.code, "policy_denied_command_cwd_outside_repo");
    assert_eq!(
        cwd_error.class,
        cadence_desktop_lib::commands::CommandErrorClass::PolicyDenied
    );
}

#[cfg(unix)]
#[test]
fn tool_runtime_search_and_find_skip_symlink_escapes() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let outside = root.path().join("outside.txt");
    fs::write(&outside, "needle\n").expect("seed outside file");
    symlink(&outside, repo_root.join("linked.txt")).expect("create escape symlink");
    fs::write(repo_root.join("src").join("inside.txt"), "needle\n").expect("seed inside file");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let search = runtime
        .search(AutonomousSearchRequest {
            query: "needle".into(),
            path: None,
        })
        .expect("search should skip symlink escapes");
    match search.output {
        AutonomousToolOutput::Search(output) => {
            assert_eq!(output.matches.len(), 1);
            assert_eq!(output.matches[0].path, "src/inside.txt");
        }
        other => panic!("unexpected symlink-skip search output: {other:?}"),
    }

    let find = runtime
        .find(AutonomousFindRequest {
            pattern: "**/*.txt".into(),
            path: None,
        })
        .expect("find should skip symlink escapes");
    match find.output {
        AutonomousToolOutput::Find(output) => {
            assert!(output.matches.contains(&"src/inside.txt".to_string()));
            assert!(
                !output.matches.contains(&"linked.txt".to_string()),
                "find results should exclude symlink escapes: {:?}",
                output.matches
            );
        }
        other => panic!("unexpected symlink-skip find output: {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn tool_runtime_search_reports_unreadable_directories() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let blocked_dir = repo_root.join("blocked");
    fs::create_dir_all(&blocked_dir).expect("create blocked dir");
    fs::write(blocked_dir.join("hidden.txt"), "needle\n").expect("seed blocked file");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let mut permissions = fs::metadata(&blocked_dir)
        .expect("blocked dir metadata")
        .permissions();
    permissions.set_mode(0o000);
    fs::set_permissions(&blocked_dir, permissions).expect("lock blocked dir");

    let error = runtime
        .search(AutonomousSearchRequest {
            query: "needle".into(),
            path: None,
        })
        .expect_err("unreadable directories should fail deterministically");

    let mut restore = fs::metadata(&blocked_dir)
        .expect("blocked dir metadata for restore")
        .permissions();
    restore.set_mode(0o755);
    fs::set_permissions(&blocked_dir, restore).expect("restore blocked dir permissions");

    assert_eq!(error.code, "autonomous_tool_search_read_dir_failed");
    assert!(error.retryable);
}

#[test]
fn tool_runtime_returns_project_not_found_for_unknown_projects() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));

    let error = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        "missing-project",
    )
    .expect_err("unknown projects should not resolve a repo root");
    assert_eq!(error.code, "project_not_found");
}

#[cfg(unix)]
#[test]
fn tool_runtime_denies_symlink_escapes() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let outside = root.path().join("outside.txt");
    fs::write(&outside, "outside\n").expect("seed outside file");
    symlink(&outside, repo_root.join("linked.txt")).expect("create escape symlink");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let error = runtime
        .read(AutonomousReadRequest {
            path: "linked.txt".into(),
            start_line: None,
            line_count: None,
        })
        .expect_err("symlink escape should be denied");
    assert_eq!(error.code, "autonomous_tool_path_denied");
    assert_eq!(
        error.class,
        cadence_desktop_lib::commands::CommandErrorClass::PolicyDenied
    );
}
