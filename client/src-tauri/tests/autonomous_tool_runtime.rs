use std::{
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    thread,
};

use git2::{IndexAddOption, Repository, Signature};
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        RepositoryDiffScope, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlStateDto, RuntimeRunPendingControlSnapshotDto,
    },
    configure_builder_with_state, db,
    git::{diff::MAX_PATCH_BYTES, repository::CanonicalRepository},
    mcp::{
        persist_mcp_registry, McpConnectionState, McpConnectionStatus, McpEnvironmentReference,
        McpRegistry, McpServerRecord, McpTransport,
    },
    registry::{self, RegistryProjectRecord},
    runtime::{
        AutonomousCodeIntelAction, AutonomousCodeIntelRequest, AutonomousCommandPolicyOutcome,
        AutonomousCommandRequest, AutonomousEditRequest, AutonomousFindRequest,
        AutonomousGitDiffRequest, AutonomousGitStatusRequest, AutonomousLspAction,
        AutonomousLspRequest, AutonomousMacosAutomationAction, AutonomousMacosAutomationRequest,
        AutonomousMcpAction, AutonomousMcpRequest, AutonomousNotebookEditRequest,
        AutonomousProcessManagerAction, AutonomousProcessManagerRequest,
        AutonomousProcessOwnershipScope, AutonomousReadMode, AutonomousReadRequest,
        AutonomousSearchRequest, AutonomousSubagentRequest, AutonomousSubagentType,
        AutonomousTodoAction, AutonomousTodoRequest, AutonomousToolAccessAction,
        AutonomousToolAccessRequest, AutonomousToolOutput, AutonomousToolRequest,
        AutonomousToolRuntime, AutonomousToolRuntimeLimits, AutonomousToolSearchRequest,
        AutonomousWebConfig, AutonomousWebFetchContentKind, AutonomousWebFetchRequest,
        AutonomousWebSearchProviderConfig, AutonomousWebSearchRequest, AutonomousWriteRequest,
    },
    state::DesktopState,
};

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    DesktopState::default()
        .with_global_db_path_override(root.path().join("app-data").join("xero.db"))
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
        last_commit: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
        additions: 0,
        deletions: 0,
    };

    let registry_path = app
        .state::<DesktopState>()
        .global_db_path(&app.handle().clone())
        .expect("registry path");
    db::configure_project_database_paths(&registry_path);
    db::import_project(&repository, app.state::<DesktopState>().import_failpoints())
        .expect("import project into app-data db");

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
            provider_profile_id: None,
            model_id: "model-1".into(),
            thinking_effort: None,
            approval_mode: active,
            plan_mode_required: false,
            revision: 1,
            applied_at: "2026-04-22T00:00:00Z".into(),
        },
        pending: pending.map(|approval_mode| RuntimeRunPendingControlSnapshotDto {
            provider_profile_id: None,
            model_id: "model-1".into(),
            thinking_effort: None,
            approval_mode,
            plan_mode_required: false,
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

fn search_request(query: impl Into<String>, path: Option<&str>) -> AutonomousSearchRequest {
    AutonomousSearchRequest {
        query: query.into(),
        path: path.map(str::to_owned),
        regex: false,
        ignore_case: false,
        include_hidden: false,
        include_ignored: false,
        include_globs: Vec::new(),
        exclude_globs: Vec::new(),
        context_lines: None,
        max_results: None,
    }
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

fn spawn_mcp_http_server(sse_result: bool) -> String {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test mcp http server");
    let address = listener.local_addr().expect("test mcp http server addr");

    thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().expect("accept test mcp request");
            let body = read_http_request_body(&mut stream);
            let value: serde_json::Value =
                serde_json::from_str(&body).unwrap_or_else(|_| serde_json::json!({}));
            let method = value
                .get("method")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default();
            let id = value.get("id").and_then(serde_json::Value::as_i64);

            if id.is_none() {
                write!(
                    stream,
                    "HTTP/1.1 202 Accepted\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n"
                )
                .expect("write notification response");
                continue;
            }

            let result = match method {
                "initialize" => serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "capabilities": {}
                }),
                "tools/list" => serde_json::json!({
                    "tools": [
                        {
                            "name": if sse_result { "sse_tool" } else { "http_tool" },
                            "description": "test tool"
                        }
                    ]
                }),
                other => serde_json::json!({ "echoed": other }),
            };
            let json = serde_json::json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": result
            })
            .to_string();
            let (content_type, response_body) = if sse_result && method == "tools/list" {
                (
                    "text/event-stream",
                    format!("event: message\ndata: {json}\n\n"),
                )
            } else {
                ("application/json", json)
            };
            write!(
                stream,
                "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nmcp-session-id: test-session\r\nContent-Length: {}\r\nConnection: keep-alive\r\n\r\n{}",
                response_body.len(),
                response_body,
            )
            .expect("write mcp response");
        }
    });

    format!("http://{address}/mcp")
}

fn read_http_request_body(stream: &mut impl Read) -> String {
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let mut content_length = 0_usize;
    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).expect("read request header");
        if bytes == 0 || line == "\r\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().expect("content length");
            }
        }
    }
    let mut body = vec![0_u8; content_length];
    reader.read_exact(&mut body).expect("read request body");
    String::from_utf8(body).expect("utf8 request body")
}

#[test]
fn tool_runtime_tool_access_lists_and_grants_requested_groups() {
    let root = tempfile::tempdir().expect("temp dir");
    let runtime = AutonomousToolRuntime::new(root.path()).expect("runtime");

    let list = runtime
        .execute(AutonomousToolRequest::ToolAccess(
            AutonomousToolAccessRequest {
                action: AutonomousToolAccessAction::List,
                groups: Vec::new(),
                tools: Vec::new(),
                reason: None,
            },
        ))
        .expect("list tool access groups");
    match list.output {
        AutonomousToolOutput::ToolAccess(output) => {
            assert_eq!(output.action, "list");
            assert!(output
                .available_groups
                .iter()
                .any(|group| group.name == "emulator"));
            assert!(output
                .available_groups
                .iter()
                .any(|group| group.name == "process_manager"
                    && group.tools == vec!["process_manager"]));
            assert!(output
                .available_groups
                .iter()
                .any(|group| group.name == "macos" && group.tools == vec!["macos_automation"]));
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let request = runtime
        .execute(AutonomousToolRequest::ToolAccess(
            AutonomousToolAccessRequest {
                action: AutonomousToolAccessAction::Request,
                groups: vec!["emulator".into(), "process_manager".into(), "macos".into()],
                tools: vec!["command".into(), "missing_tool".into()],
                reason: Some("Need to drive app use and run setup.".into()),
            },
        ))
        .expect("request tool access");
    match request.output {
        AutonomousToolOutput::ToolAccess(output) => {
            assert_eq!(output.action, "request");
            assert!(output.granted_tools.contains(&"emulator".into()));
            assert!(output.granted_tools.contains(&"process_manager".into()));
            assert!(output.granted_tools.contains(&"macos_automation".into()));
            assert!(output.granted_tools.contains(&"command".into()));
            assert!(output.denied_tools.contains(&"missing_tool".into()));
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn tool_runtime_macos_automation_checks_permissions_and_gates_control() {
    let root = tempfile::tempdir().expect("temp dir");
    let runtime = AutonomousToolRuntime::new(root.path()).expect("runtime");

    let permissions = runtime
        .execute(AutonomousToolRequest::MacosAutomation(
            AutonomousMacosAutomationRequest {
                action: AutonomousMacosAutomationAction::MacPermissions,
                app_name: None,
                bundle_id: None,
                pid: None,
                window_id: None,
                monitor_id: None,
                screenshot_target: None,
            },
        ))
        .expect("macOS permissions check should succeed");
    match permissions.output {
        AutonomousToolOutput::MacosAutomation(output) => {
            assert_eq!(
                output.action,
                AutonomousMacosAutomationAction::MacPermissions
            );
            if output.platform_supported {
                assert!(output.performed);
            }
            assert!(!output.policy.approval_required);
            assert!(!output.permissions.is_empty());
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let blocked_activate = runtime
        .execute(AutonomousToolRequest::MacosAutomation(
            AutonomousMacosAutomationRequest {
                action: AutonomousMacosAutomationAction::MacAppActivate,
                app_name: Some("Finder".into()),
                bundle_id: None,
                pid: None,
                window_id: None,
                monitor_id: None,
                screenshot_target: None,
            },
        ))
        .expect("macOS app activation should return approval boundary");
    match blocked_activate.output {
        AutonomousToolOutput::MacosAutomation(output) => {
            assert_eq!(
                output.action,
                AutonomousMacosAutomationAction::MacAppActivate
            );
            assert!(!output.performed);
            assert!(output.policy.approval_required);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let blocked_screenshot = runtime
        .execute(AutonomousToolRequest::MacosAutomation(
            AutonomousMacosAutomationRequest {
                action: AutonomousMacosAutomationAction::MacScreenshot,
                app_name: None,
                bundle_id: None,
                pid: None,
                window_id: None,
                monitor_id: None,
                screenshot_target: None,
            },
        ))
        .expect("macOS screenshot should return approval boundary");
    match blocked_screenshot.output {
        AutonomousToolOutput::MacosAutomation(output) => {
            assert_eq!(
                output.action,
                AutonomousMacosAutomationAction::MacScreenshot
            );
            assert!(!output.performed);
            assert!(output.policy.approval_required);
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn tool_runtime_process_manager_phase_four_controls_owned_processes() {
    let root = tempfile::tempdir().expect("temp dir");
    let runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_runtime_run_controls(runtime_control_state(RuntimeRunApprovalModeDto::Yolo, None));

    let start = runtime
        .execute(AutonomousToolRequest::ProcessManager(
            AutonomousProcessManagerRequest {
                action: AutonomousProcessManagerAction::Start,
                process_id: None,
                pid: None,
                parent_pid: None,
                port: None,
                group: Some("dev".into()),
                label: Some("test watcher".into()),
                process_type: Some("test_watcher".into()),
                argv: shell_argv(runtime_shell::script_print_line_and_sleep("ready", 30)),
                cwd: None,
                shell_mode: false,
                interactive: false,
                target_ownership: None,
                persistent: false,
                timeout_ms: None,
                after_cursor: None,
                since_last_read: false,
                max_bytes: None,
                tail_lines: None,
                stream: None,
                filter: None,
                input: None,
                wait_pattern: None,
                wait_port: None,
                wait_url: None,
                signal: None,
            },
        ))
        .expect("phase-three process manager start");
    let process_id = match start.output {
        AutonomousToolOutput::ProcessManager(output) => {
            assert_eq!(output.action, AutonomousProcessManagerAction::Start);
            assert_eq!(output.phase, "phase_5_system_process_visibility");
            assert!(output.spawned);
            assert_eq!(output.processes.len(), 1);
            assert!(output
                .contract
                .supported_actions
                .contains(&AutonomousProcessManagerAction::Kill));
            assert!(output.contract.persistence.redact_before_persistence);
            assert_eq!(
                output.contract.output_limits.cursor_kind,
                "monotonic_output_cursor"
            );
            output.process_id.expect("process id")
        }
        other => panic!("unexpected output: {other:?}"),
    };

    let mut saw_ready = false;
    for _ in 0..20 {
        let output = runtime
            .execute(AutonomousToolRequest::ProcessManager(
                AutonomousProcessManagerRequest {
                    action: AutonomousProcessManagerAction::Output,
                    process_id: Some(process_id.clone()),
                    pid: None,
                    parent_pid: None,
                    port: None,
                    group: None,
                    label: None,
                    process_type: None,
                    argv: Vec::new(),
                    cwd: None,
                    shell_mode: false,
                    interactive: false,
                    target_ownership: None,
                    persistent: false,
                    timeout_ms: None,
                    after_cursor: None,
                    since_last_read: false,
                    max_bytes: None,
                    tail_lines: None,
                    stream: None,
                    filter: None,
                    input: None,
                    wait_pattern: None,
                    wait_port: None,
                    wait_url: None,
                    signal: None,
                },
            ))
            .expect("phase-three process manager output");
        match output.output {
            AutonomousToolOutput::ProcessManager(output) => {
                saw_ready = output
                    .chunks
                    .iter()
                    .filter_map(|chunk| chunk.text.as_deref())
                    .any(|text| text.contains("ready"));
                if saw_ready {
                    break;
                }
            }
            other => panic!("unexpected output: {other:?}"),
        }
        thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(saw_ready, "expected process output to include ready line");

    let kill = runtime
        .execute(AutonomousToolRequest::ProcessManager(
            AutonomousProcessManagerRequest {
                action: AutonomousProcessManagerAction::Kill,
                process_id: Some(process_id.clone()),
                pid: None,
                parent_pid: None,
                port: None,
                group: None,
                label: None,
                process_type: None,
                argv: Vec::new(),
                cwd: None,
                shell_mode: false,
                interactive: false,
                target_ownership: None,
                persistent: false,
                timeout_ms: None,
                after_cursor: None,
                since_last_read: false,
                max_bytes: None,
                tail_lines: None,
                stream: None,
                filter: None,
                input: None,
                wait_pattern: None,
                wait_port: None,
                wait_url: None,
                signal: None,
            },
        ))
        .expect("phase-three process manager kill");
    match kill.output {
        AutonomousToolOutput::ProcessManager(output) => {
            assert_eq!(output.process_id.as_deref(), Some(process_id.as_str()));
            assert!(output.spawned);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let external_kill = runtime
        .execute(AutonomousToolRequest::ProcessManager(
            AutonomousProcessManagerRequest {
                action: AutonomousProcessManagerAction::Kill,
                process_id: Some("external-1".into()),
                pid: None,
                parent_pid: None,
                port: None,
                group: None,
                label: None,
                process_type: None,
                argv: Vec::new(),
                cwd: None,
                shell_mode: false,
                interactive: false,
                target_ownership: Some(AutonomousProcessOwnershipScope::External),
                persistent: false,
                timeout_ms: None,
                after_cursor: None,
                since_last_read: false,
                max_bytes: None,
                tail_lines: None,
                stream: None,
                filter: None,
                input: None,
                wait_pattern: None,
                wait_port: None,
                wait_url: None,
                signal: None,
            },
        ))
        .expect_err("external kill remains out of scope in phase three");
    assert_eq!(
        external_kill.code,
        "autonomous_tool_process_manager_external_unsupported"
    );
}

#[test]
fn tool_runtime_executes_priority_one_agent_surface_tools() {
    let root = tempfile::tempdir().expect("temp dir");
    fs::create_dir_all(root.path().join("src")).expect("create src");
    fs::write(
        root.path().join("src").join("lib.rs"),
        "pub struct Greeter;\n\npub fn greet() {}\n",
    )
    .expect("seed source");
    fs::write(
        root.path().join("work.ipynb"),
        serde_json::json!({
            "cells": [
                {
                    "cell_type": "code",
                    "metadata": {},
                    "source": ["print('old')\n"],
                    "outputs": [],
                    "execution_count": null
                }
            ],
            "metadata": {},
            "nbformat": 4,
            "nbformat_minor": 5
        })
        .to_string(),
    )
    .expect("seed notebook");

    let mcp_registry_path = root.path().join("mcp-registry.json");
    persist_mcp_registry(
        &mcp_registry_path,
        &McpRegistry {
            version: 1,
            servers: vec![McpServerRecord {
                id: "workspace-mcp".into(),
                name: "Workspace MCP".into(),
                transport: McpTransport::Stdio {
                    command: "node".into(),
                    args: vec!["server.mjs".into()],
                },
                env: Vec::new(),
                cwd: None,
                connection: McpConnectionState {
                    status: McpConnectionStatus::Connected,
                    diagnostic: None,
                    last_checked_at: Some("2026-04-25T00:00:00Z".into()),
                    last_healthy_at: Some("2026-04-25T00:00:00Z".into()),
                },
                updated_at: "2026-04-25T00:00:00Z".into(),
            }],
            updated_at: "2026-04-25T00:00:00Z".into(),
        },
    )
    .expect("persist mcp registry");

    let runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_mcp_registry_path(mcp_registry_path);

    let tool_search = runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "notebook".into(),
            limit: None,
        })
        .expect("tool search");
    match tool_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            assert!(output
                .matches
                .iter()
                .any(|item| item.tool_name == "notebook_edit"));
        }
        other => panic!("unexpected tool search output: {other:?}"),
    }
    let hash_search = runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "hash".into(),
            limit: None,
        })
        .expect("tool search for file hash");
    match hash_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            assert!(output
                .matches
                .iter()
                .any(|item| item.tool_name == "file_hash"));
        }
        other => panic!("unexpected hash tool search output: {other:?}"),
    }
    let lsp_search = runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "lsp".into(),
            limit: None,
        })
        .expect("tool search for lsp");
    match lsp_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            assert!(output
                .matches
                .iter()
                .any(|item| item.tool_name == "lsp" && item.group == "intelligence"));
        }
        other => panic!("unexpected lsp tool search output: {other:?}"),
    }
    let solana_search = runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "solana".into(),
            limit: Some(3),
        })
        .expect("tool search for deferred solana tools");
    match solana_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            assert_eq!(output.matches.len(), 3);
            assert!(output.truncated);
            assert!(output
                .matches
                .iter()
                .all(|item| item.group == "solana" && item.tool_name.starts_with("solana_")));
        }
        other => panic!("unexpected solana tool search output: {other:?}"),
    }

    let todo = runtime
        .todo(AutonomousTodoRequest {
            action: AutonomousTodoAction::Upsert,
            id: Some("inspect".into()),
            title: Some("Inspect symbols".into()),
            notes: None,
            status: None,
        })
        .expect("todo upsert");
    match todo.output {
        AutonomousToolOutput::Todo(output) => {
            assert_eq!(output.items.len(), 1);
            assert_eq!(output.items[0].id, "inspect");
        }
        other => panic!("unexpected todo output: {other:?}"),
    }

    let subagent = runtime
        .subagent(AutonomousSubagentRequest {
            agent_type: AutonomousSubagentType::Explore,
            prompt: "Find the relevant symbols.".into(),
            model_id: Some("fast-model".into()),
        })
        .expect("subagent task");
    match subagent.output {
        AutonomousToolOutput::Subagent(output) => {
            assert_eq!(output.task.subagent_id, "subagent-1");
            assert_eq!(output.task.model_id.as_deref(), Some("fast-model"));
        }
        other => panic!("unexpected subagent output: {other:?}"),
    }

    let symbols = runtime
        .code_intel(AutonomousCodeIntelRequest {
            action: AutonomousCodeIntelAction::Symbols,
            query: Some("greet".into()),
            path: Some("src".into()),
            limit: Some(10),
        })
        .expect("code intel symbols");
    match symbols.output {
        AutonomousToolOutput::CodeIntel(output) => {
            assert!(output.symbols.iter().any(|symbol| symbol.name == "greet"));
            assert!(output.diagnostics.is_empty());
        }
        other => panic!("unexpected code intel output: {other:?}"),
    }

    let lsp_servers = runtime
        .lsp(AutonomousLspRequest {
            action: AutonomousLspAction::Servers,
            query: None,
            path: None,
            limit: None,
            server_id: None,
            timeout_ms: None,
        })
        .expect("lsp servers");
    match lsp_servers.output {
        AutonomousToolOutput::Lsp(output) => {
            assert!(output
                .servers
                .iter()
                .any(|server| server.server_id == "rust_analyzer"));
            assert_eq!(output.mode, "server_catalog");
        }
        other => panic!("unexpected lsp server output: {other:?}"),
    }

    let lsp_symbols = runtime
        .lsp(AutonomousLspRequest {
            action: AutonomousLspAction::Symbols,
            query: Some("greet".into()),
            path: Some("src/lib.rs".into()),
            limit: Some(10),
            server_id: Some("rust_analyzer".into()),
            timeout_ms: Some(500),
        })
        .expect("lsp symbols");
    match lsp_symbols.output {
        AutonomousToolOutput::Lsp(output) => {
            assert!(output.symbols.iter().any(|symbol| symbol.name == "greet"));
            assert!(output.mode.starts_with("native_fallback") || output.mode == "external_lsp");
            if output.mode == "native_fallback_lsp_unavailable" {
                let suggestion = output
                    .install_suggestion
                    .as_ref()
                    .expect("missing rust analyzer should include install suggestion");
                assert_eq!(suggestion.server_id, "rust_analyzer");
                assert!(!suggestion.candidate_commands.is_empty());
            }
        }
        other => panic!("unexpected lsp symbol output: {other:?}"),
    }

    let notebook = runtime
        .notebook_edit(AutonomousNotebookEditRequest {
            path: "work.ipynb".into(),
            cell_index: 0,
            expected_source: Some("print('old')\n".into()),
            replacement_source: "print('new')\n".into(),
        })
        .expect("notebook edit");
    assert_eq!(notebook.tool_name, "notebook_edit");
    assert!(fs::read_to_string(root.path().join("work.ipynb"))
        .expect("read notebook")
        .contains("print('new')"));

    let mcp = runtime
        .mcp(AutonomousMcpRequest {
            action: AutonomousMcpAction::ListServers,
            server_id: None,
            name: None,
            uri: None,
            arguments: None,
            timeout_ms: None,
        })
        .expect("mcp list");
    match mcp.output {
        AutonomousToolOutput::Mcp(output) => {
            assert_eq!(output.servers.len(), 1);
            assert_eq!(output.servers[0].server_id, "workspace-mcp");
            assert_eq!(output.servers[0].status, "connected");
        }
        other => panic!("unexpected mcp output: {other:?}"),
    }
}

#[test]
fn tool_runtime_todo_generated_ids_do_not_collide_after_deletes() {
    let root = tempfile::tempdir().expect("temp dir");
    let runtime = AutonomousToolRuntime::new(root.path()).expect("runtime");

    for title in ["First task", "Second task"] {
        runtime
            .todo(AutonomousTodoRequest {
                action: AutonomousTodoAction::Upsert,
                id: None,
                title: Some(title.into()),
                notes: None,
                status: None,
            })
            .expect("auto todo upsert");
    }
    runtime
        .todo(AutonomousTodoRequest {
            action: AutonomousTodoAction::Delete,
            id: Some("todo-1".into()),
            title: None,
            notes: None,
            status: None,
        })
        .expect("delete first generated todo");

    let third = runtime
        .todo(AutonomousTodoRequest {
            action: AutonomousTodoAction::Upsert,
            id: None,
            title: Some("Third task".into()),
            notes: None,
            status: None,
        })
        .expect("third generated todo should not overwrite todo-2");

    match third.output {
        AutonomousToolOutput::Todo(output) => {
            assert_eq!(
                output.changed_item.as_ref().map(|item| item.id.as_str()),
                Some("todo-3")
            );
            assert_eq!(
                output
                    .items
                    .iter()
                    .map(|item| item.id.as_str())
                    .collect::<Vec<_>>(),
                vec!["todo-2", "todo-3"]
            );
        }
        other => panic!("unexpected todo output: {other:?}"),
    }
}

#[test]
fn tool_runtime_invokes_mcp_capabilities_across_transports() {
    let root = tempfile::tempdir().expect("temp dir");
    env::set_var("XERO_TEST_MCP_LEAK_SECRET", "should-not-leak");
    env::set_var("XERO_TEST_MCP_ALLOWED_SECRET", "allowed-secret");
    let stdio_server = root.path().join("mcp_stdio_server.py");
    fs::write(
        &stdio_server,
        r#"
import json
import os
import sys

def read_message():
    content_length = None
    while True:
        line = sys.stdin.buffer.readline()
        if not line:
            return None
        if line in (b"\r\n", b"\n"):
            break
        name, _, value = line.decode("utf-8").partition(":")
        if name.lower() == "content-length":
            content_length = int(value.strip())
    if content_length is None:
        return None
    return json.loads(sys.stdin.buffer.read(content_length).decode("utf-8"))

def write_message(payload):
    body = json.dumps(payload).encode("utf-8")
    sys.stdout.buffer.write(f"Content-Length: {len(body)}\r\n\r\n".encode("utf-8"))
    sys.stdout.buffer.write(body)
    sys.stdout.buffer.flush()

while True:
    message = read_message()
    if message is None:
        break
    if "id" not in message:
        continue
    method = message.get("method")
    if method == "initialize":
        result = {"protocolVersion": "2024-11-05", "capabilities": {}}
    elif method == "tools/list":
        result = {"tools": [{"name": "stdio_tool", "description": "test tool"}]}
    elif method == "tools/call":
        result = {
            "content": [
                {
                    "type": "text",
                    "text": json.dumps(
                        {
                            "allowed": os.environ.get("MCP_ALLOWED_SECRET"),
                            "leaked": os.environ.get("XERO_TEST_MCP_LEAK_SECRET"),
                            "sanitized": os.environ.get("XERO_AGENT_SANITIZED_ENV"),
                        }
                    ),
                }
            ]
        }
    else:
        result = {"echoed": method}
    write_message({"jsonrpc": "2.0", "id": message["id"], "result": result})
"#,
    )
    .expect("write stdio mcp server");

    let http_url = spawn_mcp_http_server(false);
    let sse_url = spawn_mcp_http_server(true);
    let mcp_registry_path = root.path().join("mcp-registry.json");
    persist_mcp_registry(
        &mcp_registry_path,
        &McpRegistry {
            version: 1,
            servers: vec![
                McpServerRecord {
                    id: "stdio-mcp".into(),
                    name: "Stdio MCP".into(),
                    transport: McpTransport::Stdio {
                        command: "python3".into(),
                        args: vec![stdio_server.to_string_lossy().into_owned()],
                    },
                    env: vec![McpEnvironmentReference {
                        key: "MCP_ALLOWED_SECRET".into(),
                        from_env: "XERO_TEST_MCP_ALLOWED_SECRET".into(),
                    }],
                    cwd: None,
                    connection: McpConnectionState {
                        status: McpConnectionStatus::Connected,
                        diagnostic: None,
                        last_checked_at: Some("2026-04-25T00:00:00Z".into()),
                        last_healthy_at: Some("2026-04-25T00:00:00Z".into()),
                    },
                    updated_at: "2026-04-25T00:00:00Z".into(),
                },
                McpServerRecord {
                    id: "http-mcp".into(),
                    name: "HTTP MCP".into(),
                    transport: McpTransport::Http { url: http_url },
                    env: Vec::new(),
                    cwd: None,
                    connection: McpConnectionState {
                        status: McpConnectionStatus::Connected,
                        diagnostic: None,
                        last_checked_at: Some("2026-04-25T00:00:00Z".into()),
                        last_healthy_at: Some("2026-04-25T00:00:00Z".into()),
                    },
                    updated_at: "2026-04-25T00:00:00Z".into(),
                },
                McpServerRecord {
                    id: "sse-mcp".into(),
                    name: "SSE MCP".into(),
                    transport: McpTransport::Sse { url: sse_url },
                    env: Vec::new(),
                    cwd: None,
                    connection: McpConnectionState {
                        status: McpConnectionStatus::Connected,
                        diagnostic: None,
                        last_checked_at: Some("2026-04-25T00:00:00Z".into()),
                        last_healthy_at: Some("2026-04-25T00:00:00Z".into()),
                    },
                    updated_at: "2026-04-25T00:00:00Z".into(),
                },
            ],
            updated_at: "2026-04-25T00:00:00Z".into(),
        },
    )
    .expect("persist mcp registry");

    let runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_mcp_registry_path(mcp_registry_path);

    for (server_id, expected_tool) in [
        ("stdio-mcp", "stdio_tool"),
        ("http-mcp", "http_tool"),
        ("sse-mcp", "sse_tool"),
    ] {
        let result = runtime
            .mcp(AutonomousMcpRequest {
                action: AutonomousMcpAction::ListTools,
                server_id: Some(server_id.into()),
                name: None,
                uri: None,
                arguments: None,
                timeout_ms: Some(5_000),
            })
            .expect("list mcp tools");
        match result.output {
            AutonomousToolOutput::Mcp(output) => {
                let tools = output
                    .result
                    .expect("mcp list tools result")
                    .get("tools")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .expect("tools array");
                assert!(tools.iter().any(|tool| tool["name"] == expected_tool));
            }
            other => panic!("unexpected mcp output: {other:?}"),
        }
    }

    let env_result = runtime
        .mcp(AutonomousMcpRequest {
            action: AutonomousMcpAction::InvokeTool,
            server_id: Some("stdio-mcp".into()),
            name: Some("env".into()),
            uri: None,
            arguments: Some(serde_json::json!({})),
            timeout_ms: Some(5_000),
        })
        .expect("invoke stdio mcp tool with explicit env mapping");
    match env_result.output {
        AutonomousToolOutput::Mcp(output) => {
            let result = output.result.expect("mcp tool call result");
            let text = result
                .get("content")
                .and_then(serde_json::Value::as_array)
                .and_then(|items| items.first())
                .and_then(|item| item.get("text"))
                .and_then(serde_json::Value::as_str)
                .expect("text content with env report");
            let env_report: serde_json::Value =
                serde_json::from_str(text).expect("env report json");
            assert_eq!(env_report["allowed"], "allowed-secret");
            assert_eq!(env_report["sanitized"], "1");
            assert!(env_report["leaked"].is_null());
        }
        other => panic!("unexpected mcp env output: {other:?}"),
    }
    env::remove_var("XERO_TEST_MCP_LEAK_SECRET");
    env::remove_var("XERO_TEST_MCP_ALLOWED_SECRET");
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
            system_path: false,
            mode: None,
            start_line: Some(2),
            line_count: Some(2),
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
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
        .search(search_request("beta", Some("src")))
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
            content: "hello from Xero\n".into(),
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
                "hello from Xero\n"
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
            expected_hash: None,
            start_line_hash: None,
            end_line_hash: None,
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
fn tool_runtime_dispatches_solana_tools_through_project_runtime() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let request: AutonomousToolRequest = serde_json::from_value(serde_json::json!({
        "tool": "solana_docs",
        "input": {
            "action": "catalog"
        }
    }))
    .expect("solana docs request should deserialize");

    let result = runtime
        .execute(request)
        .expect("solana docs should dispatch");
    assert_eq!(result.tool_name, "solana_docs");
    match result.output {
        AutonomousToolOutput::Solana(output) => {
            assert_eq!(output.action, "catalog");
            let value: serde_json::Value =
                serde_json::from_str(&output.value_json).expect("catalog json");
            assert!(value.as_array().is_some_and(|entries| !entries.is_empty()));
        }
        other => panic!("unexpected solana output: {other:?}"),
    }
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
                    && entry.unstaged == Some(xero_desktop_lib::commands::ChangeKind::Modified)
            }));
            assert!(output.entries.iter().any(|entry| {
                entry.path == "src/staged.txt"
                    && entry.staged == Some(xero_desktop_lib::commands::ChangeKind::Added)
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
            system_path: false,
            mode: Some(AutonomousReadMode::Text),
            start_line: None,
            line_count: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        })
        .expect_err("binary reads should be rejected");
    assert_eq!(invalid_read.code, "autonomous_tool_file_not_text");

    let oversized_query = "x".repeat(257);
    let search_error = runtime
        .search(search_request(oversized_query, None))
        .expect_err("oversized search query should be rejected");
    assert_eq!(search_error.code, "autonomous_tool_search_query_too_large");

    let empty_search = runtime
        .search(search_request("missing", Some("src")))
        .expect("zero-match search should still succeed");
    match empty_search.output {
        AutonomousToolOutput::Search(output) => assert!(output.matches.is_empty()),
        other => panic!("unexpected empty-search output: {other:?}"),
    }

    let search_with_binary_and_large_files = runtime
        .search(search_request("gamma", None))
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
            expected_hash: None,
            start_line_hash: None,
            end_line_hash: None,
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
            expected_hash: None,
            start_line_hash: None,
            end_line_hash: None,
        })
        .expect("first deterministic edit succeeds");
    let deterministic_mismatch = runtime
        .edit(AutonomousEditRequest {
            path: "src/app.txt".into(),
            start_line: 2,
            end_line: 2,
            expected: "beta\n".into(),
            replacement: "delta\n".into(),
            expected_hash: None,
            start_line_hash: None,
            end_line_hash: None,
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
            argv: shell_argv(runtime_shell::script_sleep(1)),
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

    let network = runtime
        .command(AutonomousCommandRequest {
            argv: vec!["curl".into(), "https://example.com".into()],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("network-capable commands should fail closed before spawn");

    match network.output {
        AutonomousToolOutput::Command(output) => {
            assert!(!output.spawned);
            assert_eq!(output.exit_code, None);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Escalated
            );
            assert_eq!(output.policy.code, "policy_escalated_network_command");
        }
        other => panic!("unexpected network command output: {other:?}"),
    }

    let package_mutation = runtime
        .command(AutonomousCommandRequest {
            argv: vec!["pnpm".into(), "install".into()],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("package-manager mutation commands should fail closed before spawn");

    match package_mutation.output {
        AutonomousToolOutput::Command(output) => {
            assert!(!output.spawned);
            assert_eq!(output.exit_code, None);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Escalated
            );
            assert_eq!(
                output.policy.code,
                "policy_escalated_package_manager_mutation"
            );
        }
        other => panic!("unexpected package mutation command output: {other:?}"),
    }

    let package_run = runtime
        .command(AutonomousCommandRequest {
            argv: vec!["pnpm".into(), "run".into(), "deploy".into()],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("package-manager run commands should fail closed before spawn");

    match package_run.output {
        AutonomousToolOutput::Command(output) => {
            assert!(!output.spawned);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Escalated
            );
            assert_eq!(output.policy.code, "policy_escalated_package_manager_run");
        }
        other => panic!("unexpected package run command output: {other:?}"),
    }

    let package_exec = runtime
        .command(AutonomousCommandRequest {
            argv: vec!["pnpm".into(), "exec".into(), "some-tool".into()],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("package-manager exec commands should fail closed before spawn");

    match package_exec.output {
        AutonomousToolOutput::Command(output) => {
            assert!(!output.spawned);
            assert_eq!(
                output.policy.outcome,
                AutonomousCommandPolicyOutcome::Escalated
            );
            assert_eq!(output.policy.code, "policy_escalated_package_manager_exec");
        }
        other => panic!("unexpected package exec command output: {other:?}"),
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
        xero_desktop_lib::commands::CommandErrorClass::PolicyDenied
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
        .search(search_request("needle", Some("fixtures")))
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
            system_path: false,
            mode: None,
            start_line: None,
            line_count: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        })
        .expect_err("path traversal should be denied");
    assert_eq!(read_error.code, "autonomous_tool_path_denied");
    assert_eq!(
        read_error.class,
        xero_desktop_lib::commands::CommandErrorClass::PolicyDenied
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
        xero_desktop_lib::commands::CommandErrorClass::PolicyDenied
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
        xero_desktop_lib::commands::CommandErrorClass::PolicyDenied
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
        .search(search_request("needle", None))
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
fn tool_runtime_search_skips_unreadable_directories() {
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

    let result = runtime
        .search(search_request("needle", None))
        .expect("unreadable directories should be skipped deterministically");

    let mut restore = fs::metadata(&blocked_dir)
        .expect("blocked dir metadata for restore")
        .permissions();
    restore.set_mode(0o755);
    fs::set_permissions(&blocked_dir, restore).expect("restore blocked dir permissions");

    match result.output {
        AutonomousToolOutput::Search(output) => {
            assert!(output.matches.is_empty());
            assert_eq!(output.total_matches, Some(0));
        }
        other => panic!("unexpected unreadable-dir search output: {other:?}"),
    }
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
            system_path: false,
            mode: None,
            start_line: None,
            line_count: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        })
        .expect_err("symlink escape should be denied");
    assert_eq!(error.code, "autonomous_tool_path_denied");
    assert_eq!(
        error.class,
        xero_desktop_lib::commands::CommandErrorClass::PolicyDenied
    );
}
