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
use xero_agent_core::DomainToolPackHealthStatus;
use xero_desktop_lib::{
    commands::{
        RepositoryDiffScope, RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto,
        RuntimeRunApprovalModeDto, RuntimeRunControlStateDto, RuntimeRunPendingControlSnapshotDto,
    },
    configure_builder_with_state, db,
    git::{diff::MAX_PATCH_BYTES, repository::CanonicalRepository},
    mcp::{
        persist_mcp_registry, McpConnectionState, McpConnectionStatus, McpEnvironmentReference,
        McpRegistry, McpServerRecord, McpTransport,
    },
    registry::{self, RegistryProjectRecord},
    runtime::{
        AutonomousAgentToolPolicy, AutonomousCodeIntelAction, AutonomousCodeIntelRequest,
        AutonomousCommandPolicyOutcome, AutonomousCommandPolicyProfile, AutonomousCommandRequest,
        AutonomousCopyRequest, AutonomousDeleteRequest, AutonomousDirectoryDigestHashMode,
        AutonomousDirectoryDigestRequest, AutonomousEditRequest, AutonomousFindMode,
        AutonomousFindRequest, AutonomousFsTransactionAction, AutonomousFsTransactionOperation,
        AutonomousFsTransactionRequest, AutonomousGitDiffRequest, AutonomousGitStatusRequest,
        AutonomousHashRequest, AutonomousListRequest, AutonomousListSortBy,
        AutonomousListSortDirection, AutonomousListTreeRequest, AutonomousLspAction,
        AutonomousLspRequest, AutonomousMacosAutomationAction, AutonomousMacosAutomationRequest,
        AutonomousMcpAction, AutonomousMcpRequest, AutonomousMkdirRequest,
        AutonomousNotebookEditRequest, AutonomousPatchOperation, AutonomousPatchRequest,
        AutonomousProcessManagerAction, AutonomousProcessManagerRequest,
        AutonomousProcessOwnershipScope, AutonomousReadManyRequest, AutonomousReadMode,
        AutonomousReadRequest, AutonomousRenameRequest, AutonomousSearchRequest,
        AutonomousStatKind, AutonomousStatRequest, AutonomousStructuredEditAction,
        AutonomousStructuredEditFormat, AutonomousStructuredEditOperation,
        AutonomousStructuredEditRequest, AutonomousSubagentAction, AutonomousSubagentLimits,
        AutonomousSubagentRequest, AutonomousSubagentRole, AutonomousTodoAction,
        AutonomousTodoRequest, AutonomousToolAccessAction, AutonomousToolAccessRequest,
        AutonomousToolOutput, AutonomousToolRequest, AutonomousToolRuntime,
        AutonomousToolRuntimeLimits, AutonomousToolSearchRequest, AutonomousWebConfig,
        AutonomousWebFetchContentKind, AutonomousWebFetchRequest,
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
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            provider_profile_id: None,
            model_id: "model-1".into(),
            thinking_effort: None,
            approval_mode: active,
            plan_mode_required: false,
            revision: 1,
            applied_at: "2026-04-22T00:00:00Z".into(),
        },
        pending: pending.map(|approval_mode| RuntimeRunPendingControlSnapshotDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
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
        files_only: false,
        cursor: None,
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
        for _ in 0..12 {
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
                            "description": "test tool",
                            "inputSchema": {
                                "type": "object",
                                "properties": {
                                    "message": { "type": "string" }
                                },
                                "required": ["message"]
                            }
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
    let runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_runtime_run_controls(runtime_control_state(RuntimeRunApprovalModeDto::Yolo, None));

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
            assert!(!output
                .available_groups
                .iter()
                .any(|group| group.name == "emulator"));
            assert!(output
                .available_groups
                .iter()
                .any(|group| group.name == "process_manager"
                    && group.tools == vec!["process_manager"]));
            let core_group = output
                .available_groups
                .iter()
                .find(|group| group.name == "core")
                .expect("core group");
            assert!(!core_group.tools.contains(&"harness_runner".into()));
            let read_summary = core_group
                .tool_summaries
                .iter()
                .find(|tool| tool.tool_name == "read")
                .expect("read summary");
            assert_eq!(read_summary.effect_class, "observe");
            assert_eq!(read_summary.risk_class, "observe");
            assert!(read_summary.runtime_available);
            assert!(read_summary.allowed_for_agent);
            assert!(read_summary.activation_groups.contains(&"core".into()));
            #[cfg(target_os = "macos")]
            assert!(output
                .available_groups
                .iter()
                .any(|group| group.name == "macos" && group.tools == vec!["macos_automation"]));
            assert!(output.available_groups.iter().any(|group| {
                group.name == "web_search_only"
                    && group.tools == vec!["web_search"]
                    && group.risk_class == "network"
            }));
            assert!(!output
                .available_groups
                .iter()
                .any(|group| group.name == "browser_observe"));
            assert!(output.available_groups.iter().any(|group| {
                group.name == "command_readonly"
                    && group.tools == vec!["command_probe", "command_verify"]
            }));
            assert!(output
                .available_groups
                .iter()
                .any(|group| { group.name == "mcp_list" && group.tools == vec!["mcp_list"] }));
            assert!(output
                .available_tool_packs
                .iter()
                .any(|pack| pack.pack_id == "project_context"));
            let browser_health = output
                .tool_pack_health
                .iter()
                .find(|report| report.pack_id == "browser")
                .expect("browser tool-pack health");
            assert_eq!(browser_health.status, DomainToolPackHealthStatus::Skipped);
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let request = runtime
        .execute(AutonomousToolRequest::ToolAccess(
            AutonomousToolAccessRequest {
                action: AutonomousToolAccessAction::Request,
                groups: vec![
                    "emulator".into(),
                    "process_manager".into(),
                    "macos".into(),
                    "web_search_only".into(),
                    "command_session".into(),
                ],
                tools: vec![
                    "command_run".into(),
                    "solana_alt".into(),
                    "missing_tool".into(),
                ],
                reason: Some("Need to drive app use and run setup.".into()),
            },
        ))
        .expect("request tool access");
    match request.output {
        AutonomousToolOutput::ToolAccess(output) => {
            assert_eq!(output.action, "request");
            assert!(output.granted_tools.contains(&"process_manager".into()));
            #[cfg(target_os = "macos")]
            assert!(output.granted_tools.contains(&"macos_automation".into()));
            #[cfg(not(target_os = "macos"))]
            assert!(output.denied_tools.contains(&"macos".into()));
            assert!(output.granted_tools.contains(&"web_search".into()));
            assert!(output.granted_tools.contains(&"command_session".into()));
            assert!(output.granted_tools.contains(&"command_run".into()));
            let command_run = output
                .granted_tool_details
                .iter()
                .find(|tool| tool.tool_name == "command_run")
                .expect("command_run details");
            assert_eq!(command_run.effect_class, "command");
            assert_eq!(command_run.risk_class, "command");
            assert!(command_run.runtime_available);
            assert!(output.denied_tools.contains(&"emulator".into()));
            assert!(output.denied_tools.contains(&"solana_alt".into()));
            assert!(output.denied_tools.contains(&"missing_tool".into()));
        }
        other => panic!("unexpected output: {other:?}"),
    }
}

#[test]
fn ask_tool_access_filters_and_denies_non_observe_tools() {
    let root = tempfile::tempdir().expect("temp dir");
    let mut controls = runtime_control_state(RuntimeRunApprovalModeDto::Suggest, None);
    controls.active.runtime_agent_id = RuntimeAgentIdDto::Ask;
    let runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_runtime_run_controls(controls);

    let list = runtime
        .execute(AutonomousToolRequest::ToolAccess(
            AutonomousToolAccessRequest {
                action: AutonomousToolAccessAction::List,
                groups: Vec::new(),
                tools: Vec::new(),
                reason: None,
            },
        ))
        .expect("list ask tool access groups");
    match list.output {
        AutonomousToolOutput::ToolAccess(output) => {
            assert!(output.available_groups.iter().any(|group| {
                group.name == "core"
                    && group.tools.contains(&"read".to_string())
                    && group.tools.contains(&"stat".to_string())
            }));
            assert!(!output
                .available_groups
                .iter()
                .any(|group| group.name == "command"));
            assert!(!output
                .available_groups
                .iter()
                .any(|group| group.name == "mutation"));
            assert!(!output
                .available_groups
                .iter()
                .any(|group| group.name == "browser_control"));
            assert!(!output
                .available_groups
                .iter()
                .any(|group| group.name == "agent_ops"));
        }
        other => panic!("unexpected output: {other:?}"),
    }

    let request = runtime
        .execute(AutonomousToolRequest::ToolAccess(
            AutonomousToolAccessRequest {
                action: AutonomousToolAccessAction::Request,
                groups: vec![
                    "command".into(),
                    "mutation".into(),
                    "browser_control".into(),
                ],
                tools: vec!["read".into(), "command_run".into(), "subagent".into()],
                reason: Some("Try to escape Ask observe-only mode.".into()),
            },
        ))
        .expect("request ask tool access");
    match request.output {
        AutonomousToolOutput::ToolAccess(output) => {
            assert!(output.granted_tools.contains(&"read".into()));
            assert!(!output.granted_tools.contains(&"command_run".into()));
            assert!(!output.granted_tools.contains(&"subagent".into()));
            assert!(output.denied_tools.contains(&"command_run".into()));
            assert!(output.denied_tools.contains(&"mutation".into()));
            assert!(output.denied_tools.contains(&"browser_control".into()));
            assert!(output.denied_tools.contains(&"subagent".into()));
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
        .with_mcp_registry_path(mcp_registry_path)
        .with_runtime_run_controls(runtime_control_state(RuntimeRunApprovalModeDto::Yolo, None));

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
            let hash_match = output
                .matches
                .iter()
                .find(|item| item.tool_name == "file_hash")
                .expect("file hash match");
            assert_eq!(hash_match.risk_class, "observe");
            assert_eq!(hash_match.effect_class, "observe");
            assert!(hash_match.schema_fields.contains(&"path".into()));
            assert!(hash_match.activation_tools.contains(&"file_hash".into()));
            assert!(hash_match.activation_groups.contains(&"core".into()));
            assert!(hash_match.runtime_available);
            assert!(!hash_match.why_matched.is_empty());
            assert!(hash_match.examples.len() <= 1);
            assert!(output.searched_catalog_size >= output.matches.len());
        }
        other => panic!("unexpected hash tool search output: {other:?}"),
    }
    let reserved_search = runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "harness runner".into(),
            limit: None,
        })
        .expect("tool search excludes reserved tools");
    match reserved_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            assert!(!output
                .matches
                .iter()
                .any(|item| item.tool_name == "harness_runner"));
        }
        other => panic!("unexpected reserved tool search output: {other:?}"),
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
    let command_search = runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "run tests".into(),
            limit: Some(1),
        })
        .expect("tool search for command intent");
    match command_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            assert_eq!(
                output.matches.first().map(|item| item.tool_name.as_str()),
                Some("command_verify")
            );
            assert!(output.matches[0]
                .activation_groups
                .contains(&"command_readonly".into()));
            assert!(output.matches[0]
                .why_matched
                .iter()
                .any(|reason| { reason.contains("boosted") || reason.contains("description") }));
        }
        other => panic!("unexpected command tool search output: {other:?}"),
    }
    let obscure_solana_search = runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "address lookup table".into(),
            limit: Some(1),
        })
        .expect("tool search for obscure solana capability");
    match obscure_solana_search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            let first = output.matches.first().expect("solana alt match");
            assert_eq!(first.tool_name, "solana_alt");
            assert!(first.tags.contains(&"address_lookup_table".into()));
            assert!(first.activation_groups.contains(&"solana".into()));
            assert!(first.tool_pack_ids.contains(&"solana".into()));
        }
        other => panic!("unexpected obscure solana tool search output: {other:?}"),
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
            mode: None,
            debug_stage: None,
            phase_id: None,
            phase_title: None,
            slice_id: None,
            handoff_note: None,
            evidence: None,
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
            action: AutonomousSubagentAction::Spawn,
            task_id: None,
            role: Some(AutonomousSubagentRole::Researcher),
            prompt: Some("Find the relevant symbols.".into()),
            model_id: Some("fast-model".into()),
            write_set: Vec::new(),
            decision: None,
            timeout_ms: None,
            max_tool_calls: None,
            max_tokens: None,
            max_cost_micros: None,
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
fn subagent_runtime_enforces_engineer_ownership_and_integration_decisions() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = AutonomousToolRuntime::new(temp.path())
        .expect("runtime")
        .with_subagent_limits(AutonomousSubagentLimits {
            max_concurrent_child_runs: 6,
            ..AutonomousSubagentLimits::default()
        });

    let first_engineer = runtime
        .subagent(AutonomousSubagentRequest {
            action: AutonomousSubagentAction::Spawn,
            task_id: None,
            role: Some(AutonomousSubagentRole::Engineer),
            prompt: Some("Own the source root.".into()),
            model_id: None,
            write_set: vec!["src".into()],
            decision: None,
            timeout_ms: None,
            max_tool_calls: None,
            max_tokens: None,
            max_cost_micros: None,
        })
        .expect("spawn first engineer");
    match first_engineer.output {
        AutonomousToolOutput::Subagent(output) => {
            assert_eq!(output.task.status, "registered");
            assert_eq!(output.task.write_set, vec!["src"]);
        }
        other => panic!("unexpected subagent output: {other:?}"),
    }

    let overlapping_engineer = runtime.subagent(AutonomousSubagentRequest {
        action: AutonomousSubagentAction::Spawn,
        task_id: None,
        role: Some(AutonomousSubagentRole::Engineer),
        prompt: Some("Try overlapping ownership.".into()),
        model_id: None,
        write_set: vec!["src/lib.rs".into()],
        decision: None,
        timeout_ms: None,
        max_tool_calls: None,
        max_tokens: None,
        max_cost_micros: None,
    });
    assert_eq!(
        overlapping_engineer
            .expect_err("overlapping engineer should be denied")
            .code,
        "autonomous_tool_subagent_write_set_conflict"
    );

    let readonly_with_write_set = runtime.subagent(AutonomousSubagentRequest {
        action: AutonomousSubagentAction::Spawn,
        task_id: None,
        role: Some(AutonomousSubagentRole::Reviewer),
        prompt: Some("Review only.".into()),
        model_id: None,
        write_set: vec!["README.md".into()],
        decision: None,
        timeout_ms: None,
        max_tool_calls: None,
        max_tokens: None,
        max_cost_micros: None,
    });
    assert_eq!(
        readonly_with_write_set
            .expect_err("read-only role cannot own writes")
            .code,
        "autonomous_tool_subagent_readonly_write_set"
    );

    runtime
        .subagent(AutonomousSubagentRequest {
            action: AutonomousSubagentAction::Cancel,
            task_id: Some("subagent-1".into()),
            role: None,
            prompt: None,
            model_id: None,
            write_set: Vec::new(),
            decision: None,
            timeout_ms: None,
            max_tool_calls: None,
            max_tokens: None,
            max_cost_micros: None,
        })
        .expect("cancel engineer");

    let integrated = runtime
        .subagent(AutonomousSubagentRequest {
            action: AutonomousSubagentAction::Integrate,
            task_id: Some("subagent-1".into()),
            role: None,
            prompt: None,
            model_id: None,
            write_set: Vec::new(),
            decision: Some("Do not apply output; engineer was cancelled.".into()),
            timeout_ms: None,
            max_tool_calls: None,
            max_tokens: None,
            max_cost_micros: None,
        })
        .expect("integrate cancelled engineer");
    match integrated.output {
        AutonomousToolOutput::Subagent(output) => {
            assert_eq!(
                output.task.parent_decision.as_deref(),
                Some("Do not apply output; engineer was cancelled.")
            );
            assert!(output.task.integrated_at.is_some());
        }
        other => panic!("unexpected subagent output: {other:?}"),
    }
}

#[test]
fn subagent_runtime_tracks_lifecycle_and_delegated_budgets() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = AutonomousToolRuntime::new(temp.path())
        .expect("runtime")
        .with_subagent_limits(AutonomousSubagentLimits {
            max_child_agents: 2,
            max_depth: 1,
            max_concurrent_child_runs: 2,
            max_delegated_tool_calls: 5,
            max_delegated_tokens: 1_000,
            max_delegated_cost_micros: 10_000,
        });

    let planner = runtime
        .subagent(AutonomousSubagentRequest {
            action: AutonomousSubagentAction::Spawn,
            task_id: None,
            role: Some(AutonomousSubagentRole::Planner),
            prompt: Some("Plan the implementation.".into()),
            model_id: None,
            write_set: Vec::new(),
            decision: None,
            timeout_ms: None,
            max_tool_calls: Some(50),
            max_tokens: Some(50_000),
            max_cost_micros: Some(500_000),
        })
        .expect("spawn planner");
    match planner.output {
        AutonomousToolOutput::Subagent(output) => {
            assert_eq!(output.task.subagent_id, "subagent-1");
            assert_eq!(output.task.role_label, "Planner");
            assert_eq!(output.task.depth, 1);
            assert_eq!(output.task.max_tool_calls, 5);
            assert_eq!(output.task.max_tokens, 1_000);
            assert_eq!(output.task.max_cost_micros, 10_000);
            assert!(output
                .task
                .verification_contract
                .contains("actionable plan"));
        }
        other => panic!("unexpected planner output: {other:?}"),
    }

    let follow_up = runtime
        .subagent(AutonomousSubagentRequest {
            action: AutonomousSubagentAction::SendInput,
            task_id: Some("subagent-1".into()),
            role: None,
            prompt: Some("Include migration risks.".into()),
            model_id: None,
            write_set: Vec::new(),
            decision: None,
            timeout_ms: None,
            max_tool_calls: None,
            max_tokens: None,
            max_cost_micros: None,
        })
        .expect("send input");
    match follow_up.output {
        AutonomousToolOutput::Subagent(output) => {
            assert_eq!(output.task.input_log.len(), 1);
            assert_eq!(output.task.input_log[0].kind, "send_input");
        }
        other => panic!("unexpected follow-up output: {other:?}"),
    }

    runtime
        .subagent(AutonomousSubagentRequest {
            action: AutonomousSubagentAction::Wait,
            task_id: Some("subagent-1".into()),
            role: None,
            prompt: None,
            model_id: None,
            write_set: Vec::new(),
            decision: None,
            timeout_ms: Some(0),
            max_tool_calls: None,
            max_tokens: None,
            max_cost_micros: None,
        })
        .expect("wait");

    runtime
        .subagent(AutonomousSubagentRequest {
            action: AutonomousSubagentAction::Close,
            task_id: Some("subagent-1".into()),
            role: None,
            prompt: None,
            model_id: None,
            write_set: Vec::new(),
            decision: Some("Closed after reviewing the planner output.".into()),
            timeout_ms: None,
            max_tool_calls: None,
            max_tokens: None,
            max_cost_micros: None,
        })
        .expect("close planner");

    runtime
        .subagent(AutonomousSubagentRequest {
            action: AutonomousSubagentAction::Spawn,
            task_id: None,
            role: Some(AutonomousSubagentRole::Researcher),
            prompt: Some("Research options.".into()),
            model_id: None,
            write_set: Vec::new(),
            decision: None,
            timeout_ms: None,
            max_tool_calls: None,
            max_tokens: None,
            max_cost_micros: None,
        })
        .expect("spawn researcher");

    let over_budget = runtime.subagent(AutonomousSubagentRequest {
        action: AutonomousSubagentAction::Spawn,
        task_id: None,
        role: Some(AutonomousSubagentRole::Reviewer),
        prompt: Some("Review the plan.".into()),
        model_id: None,
        write_set: Vec::new(),
        decision: None,
        timeout_ms: None,
        max_tool_calls: None,
        max_tokens: None,
        max_cost_micros: None,
    });
    assert_eq!(
        over_budget.expect_err("third child should be denied").code,
        "autonomous_tool_subagent_child_budget_exceeded"
    );
}

#[test]
fn subagent_role_policies_are_least_privilege_and_parent_bounded() {
    let researcher_policy = AutonomousAgentToolPolicy::for_subagent_role(
        AutonomousSubagentRole::Researcher,
        None,
        false,
    );
    assert!(researcher_policy.allows_tool("read"));
    assert!(researcher_policy.allows_tool("code_intel"));
    assert!(!researcher_policy.allows_tool("write"));
    assert!(!researcher_policy.allows_tool("command_probe"));
    assert!(!researcher_policy.allows_tool("command_run"));
    assert!(!researcher_policy.allows_tool("subagent"));

    let parent_policy = AutonomousAgentToolPolicy::from_definition_snapshot(&serde_json::json!({
        "toolPolicy": {
            "allowedTools": ["read"]
        }
    }))
    .expect("parent policy");
    let engineer_policy = AutonomousAgentToolPolicy::for_subagent_role(
        AutonomousSubagentRole::Engineer,
        Some(&parent_policy),
        false,
    );
    assert!(engineer_policy.allows_tool("read"));
    assert!(!engineer_policy.allows_tool("write"));
    assert!(!engineer_policy.allows_tool("command_probe"));
    assert!(!engineer_policy.allows_tool("command_run"));
}

#[test]
fn custom_agent_policy_can_enable_and_disable_domain_tool_packs() {
    let browser_policy = AutonomousAgentToolPolicy::from_definition_snapshot(&serde_json::json!({
        "toolPolicy": {
            "allowedToolPacks": ["browser"],
            "deniedToolPacks": ["emulator"],
            "browserControlAllowed": true
        }
    }))
    .expect("browser policy");
    assert!(browser_policy.allows_tool("browser_observe"));
    assert!(browser_policy.allows_tool("browser_control"));
    assert!(!browser_policy.allows_tool("emulator"));
    assert!(!browser_policy.allows_tool("solana_simulate"));

    let browser_without_opt_in =
        AutonomousAgentToolPolicy::from_definition_snapshot(&serde_json::json!({
            "toolPolicy": {
                "allowedToolPacks": ["browser"],
                "browserControlAllowed": false
            }
        }))
        .expect("browser policy without opt in");
    assert!(browser_without_opt_in.allows_tool("browser_observe"));
    assert!(!browser_without_opt_in.allows_tool("browser_control"));

    let solana_policy = AutonomousAgentToolPolicy::from_definition_snapshot(&serde_json::json!({
        "toolPolicy": {
            "allowedToolPacks": ["solana"],
            "externalServiceAllowed": true,
            "commandAllowed": true,
            "destructiveWriteAllowed": true
        }
    }))
    .expect("solana policy");
    assert!(solana_policy.allows_tool("solana_simulate"));
    assert!(solana_policy.allows_tool("solana_deploy"));
}

#[test]
fn subagent_runtime_supports_phase_nine_role_scenarios() {
    let temp = tempfile::tempdir().expect("temp dir");
    let runtime = AutonomousToolRuntime::new(temp.path())
        .expect("runtime")
        .with_subagent_limits(AutonomousSubagentLimits {
            max_concurrent_child_runs: 6,
            ..AutonomousSubagentLimits::default()
        });

    let spawn = |role, prompt: &str, write_set: Vec<&str>| {
        runtime
            .subagent(AutonomousSubagentRequest {
                action: AutonomousSubagentAction::Spawn,
                task_id: None,
                role: Some(role),
                prompt: Some(prompt.into()),
                model_id: None,
                write_set: write_set.into_iter().map(str::to_owned).collect(),
                decision: None,
                timeout_ms: None,
                max_tool_calls: None,
                max_tokens: None,
                max_cost_micros: None,
            })
            .expect("spawn role scenario")
    };

    let research = spawn(
        AutonomousSubagentRole::Researcher,
        "Research the migration approach.",
        Vec::new(),
    );
    let implement = spawn(
        AutonomousSubagentRole::Engineer,
        "Implement the researched path.",
        vec!["src/phase9"],
    );
    let debug = spawn(
        AutonomousSubagentRole::Debugger,
        "Debug the failing verification.",
        vec!["tests/phase9"],
    );
    let verify = spawn(
        AutonomousSubagentRole::Reviewer,
        "Verify the implementation evidence.",
        Vec::new(),
    );
    let plan = spawn(
        AutonomousSubagentRole::Planner,
        "Plan the engineer handoff.",
        Vec::new(),
    );

    let outputs = [research, implement, debug, verify, plan];
    let roles = outputs
        .into_iter()
        .map(|result| match result.output {
            AutonomousToolOutput::Subagent(output) => output.task.role,
            other => panic!("unexpected subagent output: {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(
        roles,
        vec![
            AutonomousSubagentRole::Researcher,
            AutonomousSubagentRole::Engineer,
            AutonomousSubagentRole::Debugger,
            AutonomousSubagentRole::Reviewer,
            AutonomousSubagentRole::Planner,
        ]
    );
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
                mode: None,
                debug_stage: None,
                phase_id: None,
                phase_title: None,
                slice_id: None,
                handoff_note: None,
                evidence: None,
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
            mode: None,
            debug_stage: None,
            phase_id: None,
            phase_title: None,
            slice_id: None,
            handoff_note: None,
            evidence: None,
        })
        .expect("delete first generated todo");

    let third = runtime
        .todo(AutonomousTodoRequest {
            action: AutonomousTodoAction::Upsert,
            id: None,
            title: Some("Third task".into()),
            notes: None,
            status: None,
            mode: None,
            debug_stage: None,
            phase_id: None,
            phase_title: None,
            slice_id: None,
            handoff_note: None,
            evidence: None,
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
        .with_mcp_registry_path(mcp_registry_path)
        .with_runtime_run_controls(runtime_control_state(RuntimeRunApprovalModeDto::Yolo, None));

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
fn tool_search_projects_mcp_tools_as_exact_dynamic_capabilities() {
    let root = tempfile::tempdir().expect("temp dir");
    let http_url = spawn_mcp_http_server(false);
    let mcp_registry_path = root.path().join("mcp-registry.json");
    persist_mcp_registry(
        &mcp_registry_path,
        &McpRegistry {
            version: 1,
            servers: vec![McpServerRecord {
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
            }],
            updated_at: "2026-04-25T00:00:00Z".into(),
        },
    )
    .expect("persist mcp registry");

    let runtime = AutonomousToolRuntime::new(root.path())
        .expect("runtime")
        .with_mcp_registry_path(mcp_registry_path)
        .with_runtime_run_controls(runtime_control_state(RuntimeRunApprovalModeDto::Yolo, None));
    let search = runtime
        .tool_search(AutonomousToolSearchRequest {
            query: "http_tool".into(),
            limit: None,
        })
        .expect("search mcp tools");
    let dynamic_tool = match search.output {
        AutonomousToolOutput::ToolSearch(output) => {
            let item = output
                .matches
                .iter()
                .find(|item| {
                    item.catalog_kind == "mcp_tool" && item.source.as_deref() == Some("http-mcp")
                })
                .expect("dynamic mcp tool search result");
            assert!(item.tool_name.starts_with("mcp__http_mcp__http_tool__"));
            assert_eq!(item.group, "mcp");
            assert_eq!(item.trust.as_deref(), Some("connected_mcp_server"));
            assert_eq!(item.approval_status.as_deref(), Some("allowed"));
            assert_eq!(item.risk_class, "external_capability_invoke");
            assert!(item.schema_fields.contains(&"message".into()));
            assert_eq!(item.activation_tools, vec![item.tool_name.clone()]);
            item.tool_name.clone()
        }
        other => panic!("unexpected tool search output: {other:?}"),
    };

    let access = runtime
        .tool_access(AutonomousToolAccessRequest {
            action: AutonomousToolAccessAction::Request,
            groups: Vec::new(),
            tools: vec![dynamic_tool.clone()],
            reason: Some("activate exact MCP tool".into()),
        })
        .expect("request exact mcp tool");
    match access.output {
        AutonomousToolOutput::ToolAccess(output) => {
            assert_eq!(output.granted_tools, vec![dynamic_tool]);
            assert!(output.denied_tools.is_empty());
        }
        other => panic!("unexpected tool access output: {other:?}"),
    }
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
            cursor: None,
            around_pattern: None,
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
            mode: None,
            path: Some("src".into()),
            max_depth: None,
            max_results: None,
            cursor: None,
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

    let bounded_find = runtime
        .find(AutonomousFindRequest {
            pattern: "**/*.txt".into(),
            mode: None,
            path: Some("src".into()),
            max_depth: Some(1),
            max_results: None,
            cursor: None,
        })
        .expect("depth-bounded find repo files");
    match bounded_find.output {
        AutonomousToolOutput::Find(output) => {
            assert_eq!(output.scope.as_deref(), Some("src"));
            assert_eq!(output.matches, vec!["src/app.txt", "src/tracked.txt"]);
            assert_eq!(output.scanned_files, 2);
            assert!(output.truncated);
        }
        other => panic!("unexpected bounded find output: {other:?}"),
    }

    let written = runtime
        .write(AutonomousWriteRequest {
            path: "notes/output.txt".into(),
            content: "hello from Xero\n".into(),
            expected_hash: None,
            create_only: false,
            overwrite: None,
            preview: false,
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
            preview: false,
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
fn tool_runtime_read_supports_cursors_pattern_windows_and_omission_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let long_text = (1..=6)
        .map(|line| format!("line {line}\n"))
        .collect::<String>();
    fs::write(repo_root.join("src").join("long.txt"), long_text).expect("seed long file");
    let window_text = (1..=30)
        .map(|line| {
            if line == 20 {
                "line 20 NEEDLE\n".to_string()
            } else {
                format!("line {line}\n")
            }
        })
        .collect::<String>();
    fs::write(repo_root.join("src").join("window.txt"), window_text).expect("seed window file");
    fs::write(
        repo_root.join("src").join("bundle.min.js"),
        format!("const bundle = \"{}\";", "x".repeat(20 * 1024)),
    )
    .expect("seed generated-looking file");

    let runtime =
        runtime_for_project_with_approval(&app, &project_id, RuntimeRunApprovalModeDto::Yolo);

    let first = runtime
        .read(AutonomousReadRequest {
            path: "src/long.txt".into(),
            system_path: false,
            mode: Some(AutonomousReadMode::Text),
            start_line: None,
            line_count: Some(2),
            cursor: None,
            around_pattern: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: true,
        })
        .expect("read first page");
    let next_cursor = match first.output {
        AutonomousToolOutput::Read(output) => {
            assert_eq!(output.path_kind, AutonomousStatKind::File);
            assert_eq!(output.size, Some(42));
            assert!(output.modified_at.is_some());
            assert_eq!(output.sha256.as_deref().map(str::len), Some(64));
            assert_eq!(output.start_line, 1);
            assert_eq!(output.content, "line 1\nline 2\n");
            assert!(output.truncated);
            assert!(output
                .cursor
                .as_deref()
                .is_some_and(|cursor| cursor.starts_with("read:v1:")));
            output.next_cursor.expect("next cursor")
        }
        other => panic!("unexpected first read output: {other:?}"),
    };

    let second = runtime
        .read(AutonomousReadRequest {
            path: "src/long.txt".into(),
            system_path: false,
            mode: Some(AutonomousReadMode::Text),
            start_line: None,
            line_count: Some(2),
            cursor: Some(next_cursor.clone()),
            around_pattern: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        })
        .expect("read continuation");
    match second.output {
        AutonomousToolOutput::Read(output) => {
            assert_eq!(output.start_line, 3);
            assert_eq!(output.content, "line 3\nline 4\n");
        }
        other => panic!("unexpected second read output: {other:?}"),
    }

    let around = runtime
        .read(AutonomousReadRequest {
            path: "src/window.txt".into(),
            system_path: false,
            mode: Some(AutonomousReadMode::Text),
            start_line: None,
            line_count: Some(5),
            cursor: None,
            around_pattern: Some("NEEDLE".into()),
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        })
        .expect("read around pattern");
    match around.output {
        AutonomousToolOutput::Read(output) => {
            assert_eq!(output.start_line, 18);
            assert!(output.content.contains("line 20 NEEDLE"));
        }
        other => panic!("unexpected around read output: {other:?}"),
    }

    let omitted = runtime
        .read(AutonomousReadRequest {
            path: "src/bundle.min.js".into(),
            system_path: false,
            mode: Some(AutonomousReadMode::Text),
            start_line: None,
            line_count: None,
            cursor: None,
            around_pattern: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        })
        .expect("omit generated-looking file");
    match omitted.output {
        AutonomousToolOutput::Read(output) => {
            assert_eq!(output.content, "");
            assert_eq!(output.line_count, 0);
            assert_eq!(
                output.content_omitted_reason.as_deref(),
                Some("minified_or_generated")
            );
            assert!(output.next_cursor.is_some());
            assert!(output.truncated);
        }
        other => panic!("unexpected omitted read output: {other:?}"),
    }

    fs::write(repo_root.join("src").join("long.txt"), "changed\n").expect("change long file");
    let stale = runtime
        .read(AutonomousReadRequest {
            path: "src/long.txt".into(),
            system_path: false,
            mode: Some(AutonomousReadMode::Text),
            start_line: None,
            line_count: Some(2),
            cursor: Some(next_cursor),
            around_pattern: None,
            byte_offset: None,
            byte_count: None,
            include_line_hashes: false,
        })
        .expect_err("stale cursor should be rejected");
    assert_eq!(stale.code, "autonomous_tool_read_cursor_stale");
}

#[test]
fn tool_runtime_search_paginates_files_only_and_reports_omissions() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = tempdir.path();
    fs::create_dir_all(root.join("src")).expect("src dir");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules dir");
    fs::write(root.join("src/a.txt"), "needle a\n").expect("a");
    fs::write(root.join("src/b.txt"), "needle b\n").expect("b");
    fs::write(root.join("src/c.txt"), "needle c\n").expect("c");
    fs::write(
        root.join("node_modules/pkg/ignored.txt"),
        "needle ignored\n",
    )
    .expect("ignored");
    fs::write(root.join("blob.bin"), [0xff_u8, 0xfe, 0x00]).expect("binary");

    let runtime = AutonomousToolRuntime::new(root).expect("runtime");
    let first = runtime
        .search(AutonomousSearchRequest {
            query: "needle".into(),
            path: None,
            regex: false,
            ignore_case: false,
            include_hidden: false,
            include_ignored: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            context_lines: None,
            max_results: Some(2),
            files_only: false,
            cursor: None,
        })
        .expect("first search page");
    let next_cursor = match first.output {
        AutonomousToolOutput::Search(output) => {
            assert_eq!(output.matches.len(), 2);
            assert_eq!(output.files.len(), 2);
            assert_eq!(output.files[0].path, "src/a.txt");
            assert_eq!(output.files[0].match_count, 1);
            assert_eq!(output.returned_matches, 2);
            assert_eq!(output.skipped_matches, 0);
            assert!(output.truncated);
            assert!(output.omissions.ignored_directories >= 1);
            assert_eq!(output.omissions.binary_files, 1);
            output.next_cursor.expect("next search cursor")
        }
        other => panic!("unexpected first search output: {other:?}"),
    };

    let second = runtime
        .search(AutonomousSearchRequest {
            query: "needle".into(),
            path: None,
            regex: false,
            ignore_case: false,
            include_hidden: false,
            include_ignored: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            context_lines: None,
            max_results: Some(2),
            files_only: false,
            cursor: Some(next_cursor.clone()),
        })
        .expect("second search page");
    match second.output {
        AutonomousToolOutput::Search(output) => {
            assert_eq!(output.skipped_matches, 2);
            assert_eq!(output.matches.len(), 1);
            assert_eq!(output.matches[0].path, "src/c.txt");
            assert!(!output.truncated);
            assert!(output.next_cursor.is_none());
        }
        other => panic!("unexpected second search output: {other:?}"),
    }

    let files_only = runtime
        .search(AutonomousSearchRequest {
            query: "needle".into(),
            path: None,
            regex: false,
            ignore_case: false,
            include_hidden: false,
            include_ignored: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            context_lines: None,
            max_results: Some(10),
            files_only: true,
            cursor: None,
        })
        .expect("files-only search");
    match files_only.output {
        AutonomousToolOutput::Search(output) => {
            assert!(output.files_only);
            assert!(output.matches.is_empty());
            assert_eq!(output.files.len(), 3);
            assert_eq!(output.returned_matches, 3);
        }
        other => panic!("unexpected files-only search output: {other:?}"),
    }

    let mismatch = runtime
        .search(AutonomousSearchRequest {
            query: "needle".into(),
            path: None,
            regex: false,
            ignore_case: true,
            include_hidden: false,
            include_ignored: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            context_lines: None,
            max_results: Some(2),
            files_only: false,
            cursor: Some(next_cursor),
        })
        .expect_err("cursor should be tied to search options");
    assert_eq!(mismatch.code, "autonomous_tool_search_cursor_mismatch");
}

#[test]
fn tool_runtime_find_supports_modes_pagination_counts_and_omissions() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = tempdir.path();
    fs::create_dir_all(root.join("src/nested")).expect("nested dir");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules dir");
    fs::write(root.join("src/app.rs"), "fn app() {}\n").expect("app");
    fs::write(root.join("src/lib.ts"), "export {}\n").expect("lib");
    fs::write(root.join("src/nested/deep.rs"), "fn deep() {}\n").expect("deep");
    fs::write(root.join("node_modules/pkg/ignored.rs"), "ignored\n").expect("ignored");

    let runtime = AutonomousToolRuntime::new(root).expect("runtime");
    let by_name = runtime
        .find(AutonomousFindRequest {
            pattern: "app.rs".into(),
            mode: Some(AutonomousFindMode::Name),
            path: None,
            max_depth: None,
            max_results: None,
            cursor: None,
        })
        .expect("find by name");
    match by_name.output {
        AutonomousToolOutput::Find(output) => {
            assert_eq!(output.mode, AutonomousFindMode::Name);
            assert_eq!(output.matches, vec!["src/app.rs"]);
            assert_eq!(output.file_count, 1);
            assert!(output.omissions.ignored_directories >= 1);
        }
        other => panic!("unexpected name find output: {other:?}"),
    }

    let first = runtime
        .find(AutonomousFindRequest {
            pattern: "rs".into(),
            mode: Some(AutonomousFindMode::Extension),
            path: None,
            max_depth: None,
            max_results: Some(1),
            cursor: None,
        })
        .expect("first extension page");
    let next_cursor = match first.output {
        AutonomousToolOutput::Find(output) => {
            assert_eq!(output.matches, vec!["src/app.rs"]);
            assert_eq!(output.returned_matches, 1);
            assert_eq!(output.skipped_matches, 0);
            assert!(output.truncated);
            output.next_cursor.expect("find cursor")
        }
        other => panic!("unexpected first extension find output: {other:?}"),
    };

    let second = runtime
        .find(AutonomousFindRequest {
            pattern: "rs".into(),
            mode: Some(AutonomousFindMode::Extension),
            path: None,
            max_depth: None,
            max_results: Some(1),
            cursor: Some(next_cursor.clone()),
        })
        .expect("second extension page");
    match second.output {
        AutonomousToolOutput::Find(output) => {
            assert_eq!(output.skipped_matches, 1);
            assert_eq!(output.matches, vec!["src/nested/deep.rs"]);
            assert!(!output.truncated);
        }
        other => panic!("unexpected second extension find output: {other:?}"),
    }

    let prefix = runtime
        .find(AutonomousFindRequest {
            pattern: "src/nested".into(),
            mode: Some(AutonomousFindMode::PathPrefix),
            path: None,
            max_depth: None,
            max_results: None,
            cursor: None,
        })
        .expect("find by path prefix");
    match prefix.output {
        AutonomousToolOutput::Find(output) => {
            assert!(output.matches.contains(&"src/nested".to_string()));
            assert!(output.matches.contains(&"src/nested/deep.rs".to_string()));
            assert!(output.directory_count >= 1);
            assert!(output.file_count >= 1);
        }
        other => panic!("unexpected path-prefix find output: {other:?}"),
    }

    let mismatch = runtime
        .find(AutonomousFindRequest {
            pattern: "rs".into(),
            mode: Some(AutonomousFindMode::Name),
            path: None,
            max_depth: None,
            max_results: Some(1),
            cursor: Some(next_cursor),
        })
        .expect_err("cursor should be tied to find mode");
    assert_eq!(mismatch.code, "autonomous_tool_find_cursor_mismatch");
}

#[test]
fn tool_runtime_list_paginates_sorts_and_reports_counts_and_omissions() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let root = tempdir.path();
    fs::create_dir_all(root.join("src/nested")).expect("nested dir");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules dir");
    fs::write(root.join("src/a.txt"), "a\n").expect("a");
    fs::write(root.join("src/b.txt"), "bbbb\n").expect("b");
    fs::write(root.join("src/nested/c.txt"), "cc\n").expect("c");
    fs::write(root.join("node_modules/pkg/ignored.txt"), "ignored\n").expect("ignored");

    let runtime = AutonomousToolRuntime::new(root).expect("runtime");
    let first = runtime
        .list(AutonomousListRequest {
            path: Some("src".into()),
            max_depth: Some(2),
            max_results: Some(2),
            sort_by: Some(AutonomousListSortBy::Size),
            sort_direction: Some(AutonomousListSortDirection::Desc),
            cursor: None,
        })
        .expect("first list page");
    let next_cursor = match first.output {
        AutonomousToolOutput::List(output) => {
            assert_eq!(output.path, "src");
            assert_eq!(output.entries.len(), 2);
            assert_eq!(output.entries[0].path, "src/b.txt");
            assert_eq!(output.entries[0].bytes, Some(5));
            assert!(output.entries[0].modified_at.is_some());
            assert_eq!(output.returned_entries, 2);
            assert_eq!(output.skipped_entries, 0);
            assert!(output.truncated);
            assert_eq!(output.file_count, 3);
            assert_eq!(output.directory_count, 1);
            output.next_cursor.expect("list cursor")
        }
        other => panic!("unexpected first list output: {other:?}"),
    };

    let second = runtime
        .list(AutonomousListRequest {
            path: Some("src".into()),
            max_depth: Some(2),
            max_results: Some(2),
            sort_by: Some(AutonomousListSortBy::Size),
            sort_direction: Some(AutonomousListSortDirection::Desc),
            cursor: Some(next_cursor.clone()),
        })
        .expect("second list page");
    match second.output {
        AutonomousToolOutput::List(output) => {
            assert_eq!(output.skipped_entries, 2);
            assert_eq!(output.entries.len(), 2);
            assert!(output.next_cursor.is_none());
        }
        other => panic!("unexpected second list output: {other:?}"),
    }

    let depth_limited = runtime
        .list(AutonomousListRequest {
            path: None,
            max_depth: Some(1),
            max_results: Some(20),
            sort_by: None,
            sort_direction: None,
            cursor: None,
        })
        .expect("depth-limited list");
    match depth_limited.output {
        AutonomousToolOutput::List(output) => {
            assert!(output.omitted.depth >= 1);
            assert!(output.omitted.ignored_directory >= 1);
            assert!(output.truncated);
        }
        other => panic!("unexpected depth-limited list output: {other:?}"),
    }

    let mismatch = runtime
        .list(AutonomousListRequest {
            path: Some("src".into()),
            max_depth: Some(2),
            max_results: Some(2),
            sort_by: Some(AutonomousListSortBy::Path),
            sort_direction: Some(AutonomousListSortDirection::Asc),
            cursor: Some(next_cursor),
        })
        .expect_err("cursor should be tied to list sort options");
    assert_eq!(mismatch.code, "autonomous_tool_list_cursor_mismatch");
}

#[test]
fn tool_runtime_write_supports_preview_and_hash_guarded_overwrite() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let repo_root = tempdir.path();
    fs::write(repo_root.join("tracked.txt"), "alpha\n").expect("seed tracked file");
    let runtime = AutonomousToolRuntime::new(repo_root).expect("runtime");

    let implicit_overwrite = runtime
        .write(AutonomousWriteRequest {
            path: "tracked.txt".into(),
            content: "beta\n".into(),
            expected_hash: None,
            create_only: false,
            overwrite: None,
            preview: false,
        })
        .expect_err("existing files require explicit overwrite");
    assert_eq!(
        implicit_overwrite.code,
        "autonomous_tool_write_overwrite_required"
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("tracked.txt")).expect("read original"),
        "alpha\n"
    );

    let current_hash = match runtime
        .hash(AutonomousHashRequest {
            path: "tracked.txt".into(),
            recursive: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_files: None,
            manifest: false,
        })
        .expect("hash current file")
        .output
    {
        AutonomousToolOutput::Hash(output) => output.sha256,
        other => panic!("unexpected hash output: {other:?}"),
    };

    let preview = runtime
        .write(AutonomousWriteRequest {
            path: "tracked.txt".into(),
            content: "beta\n".into(),
            expected_hash: Some(current_hash.clone()),
            create_only: false,
            overwrite: Some(true),
            preview: true,
        })
        .expect("preview guarded replacement");
    match preview.output {
        AutonomousToolOutput::Write(output) => {
            assert_eq!(output.path, "tracked.txt");
            assert!(!output.created);
            assert!(output.preview);
            assert!(!output.applied);
            assert_eq!(output.old_hash.as_deref(), Some(current_hash.as_str()));
            assert_eq!(output.new_hash.as_deref().map(str::len), Some(64));
            let diff = output.diff.expect("replacement diff");
            assert!(diff.contains("-alpha"));
            assert!(diff.contains("+beta"));
        }
        other => panic!("unexpected write preview output: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(repo_root.join("tracked.txt")).expect("read after preview"),
        "alpha\n"
    );

    let applied = runtime
        .write(AutonomousWriteRequest {
            path: "tracked.txt".into(),
            content: "beta\n".into(),
            expected_hash: Some(current_hash.clone()),
            create_only: false,
            overwrite: Some(true),
            preview: false,
        })
        .expect("apply guarded replacement");
    match applied.output {
        AutonomousToolOutput::Write(output) => {
            assert!(!output.created);
            assert!(!output.preview);
            assert!(output.applied);
            assert_eq!(output.old_hash.as_deref(), Some(current_hash.as_str()));
        }
        other => panic!("unexpected write output: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(repo_root.join("tracked.txt")).expect("read replaced file"),
        "beta\n"
    );

    let stale = runtime
        .write(AutonomousWriteRequest {
            path: "tracked.txt".into(),
            content: "gamma\n".into(),
            expected_hash: Some(current_hash),
            create_only: false,
            overwrite: Some(true),
            preview: false,
        })
        .expect_err("stale hash should block replacement");
    assert_eq!(stale.code, "autonomous_tool_write_expected_hash_mismatch");

    let create_only = runtime
        .write(AutonomousWriteRequest {
            path: "tracked.txt".into(),
            content: "gamma\n".into(),
            expected_hash: None,
            create_only: true,
            overwrite: Some(true),
            preview: false,
        })
        .expect_err("createOnly refuses existing files");
    assert_eq!(create_only.code, "autonomous_tool_write_create_only_exists");

    let create_preview = runtime
        .write(AutonomousWriteRequest {
            path: "notes/new.txt".into(),
            content: "new\n".into(),
            expected_hash: None,
            create_only: true,
            overwrite: None,
            preview: true,
        })
        .expect("preview create");
    match create_preview.output {
        AutonomousToolOutput::Write(output) => {
            assert!(output.created);
            assert!(output.preview);
            assert!(!output.applied);
            assert_eq!(output.content_bytes, Some(4));
            assert_eq!(output.line_count, Some(1));
            assert!(output.diff.is_none());
        }
        other => panic!("unexpected write create preview output: {other:?}"),
    }
    assert!(!repo_root.join("notes").join("new.txt").exists());
}

#[test]
fn tool_runtime_edit_supports_preview_and_conflict_context() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let repo_root = tempdir.path();
    fs::write(repo_root.join("notes.txt"), "one\ntwo\nthree\n").expect("seed notes");
    let runtime = AutonomousToolRuntime::new(repo_root).expect("runtime");

    let preview = runtime
        .edit(AutonomousEditRequest {
            path: "notes.txt".into(),
            start_line: 2,
            end_line: 2,
            expected: "two\n".into(),
            replacement: "TWO\n".into(),
            expected_hash: None,
            start_line_hash: None,
            end_line_hash: None,
            preview: true,
        })
        .expect("preview edit");
    match preview.output {
        AutonomousToolOutput::Edit(output) => {
            assert_eq!(output.path, "notes.txt");
            assert!(output.preview);
            assert!(!output.applied);
            assert_eq!(output.replacement_len, 4);
            assert_ne!(output.old_hash, output.new_hash);
            let diff = output.diff.expect("preview diff");
            assert!(diff.contains("-two"));
            assert!(diff.contains("+TWO"));
        }
        other => panic!("unexpected edit preview output: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(repo_root.join("notes.txt")).expect("read after preview"),
        "one\ntwo\nthree\n"
    );

    let conflict = runtime
        .edit(AutonomousEditRequest {
            path: "notes.txt".into(),
            start_line: 2,
            end_line: 2,
            expected: "stale\n".into(),
            replacement: "TWO\n".into(),
            expected_hash: None,
            start_line_hash: None,
            end_line_hash: None,
            preview: false,
        })
        .expect_err("mismatched expected text should include context");
    assert_eq!(conflict.code, "autonomous_tool_edit_expected_text_mismatch");
    assert!(conflict.message.contains("line 2: sha256="));
    assert!(conflict.message.contains("text=two"));
}

#[test]
fn tool_runtime_delete_previews_and_requires_digest_for_recursive_apply() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let repo_root = tempdir.path();
    fs::create_dir_all(repo_root.join("target/nested")).expect("create target tree");
    fs::write(repo_root.join("target/a.txt"), "alpha\n").expect("seed a");
    fs::write(repo_root.join("target/nested/b.txt"), "beta\n").expect("seed b");
    let runtime = AutonomousToolRuntime::new(repo_root).expect("runtime");

    let preview = runtime
        .delete(AutonomousDeleteRequest {
            path: "target".into(),
            recursive: true,
            expected_hash: None,
            expected_digest: None,
            preview: true,
        })
        .expect("preview recursive delete");
    let digest = match preview.output {
        AutonomousToolOutput::Delete(output) => {
            assert_eq!(output.path, "target");
            assert!(output.recursive);
            assert!(output.preview);
            assert!(!output.applied);
            assert_eq!(output.file_count, 2);
            assert_eq!(output.directory_count, 2);
            assert_eq!(output.deleted_count, 4);
            assert_eq!(output.bytes_estimated, 11);
            assert_eq!(output.bytes_remaining, 11);
            output.digest.expect("directory digest")
        }
        other => panic!("unexpected delete preview output: {other:?}"),
    };
    assert_eq!(digest.len(), 64);
    assert!(repo_root.join("target").exists());

    let missing_digest = runtime
        .delete(AutonomousDeleteRequest {
            path: "target".into(),
            recursive: true,
            expected_hash: None,
            expected_digest: None,
            preview: false,
        })
        .expect_err("recursive delete requires digest");
    assert_eq!(
        missing_digest.code,
        "autonomous_tool_delete_expected_digest_required"
    );

    let wrong_digest = runtime
        .delete(AutonomousDeleteRequest {
            path: "target".into(),
            recursive: true,
            expected_hash: None,
            expected_digest: Some("0".repeat(64)),
            preview: false,
        })
        .expect_err("wrong digest blocks recursive delete");
    assert_eq!(
        wrong_digest.code,
        "autonomous_tool_delete_expected_digest_mismatch"
    );

    let applied = runtime
        .delete(AutonomousDeleteRequest {
            path: "target".into(),
            recursive: true,
            expected_hash: None,
            expected_digest: Some(digest),
            preview: false,
        })
        .expect("apply digest-guarded recursive delete");
    match applied.output {
        AutonomousToolOutput::Delete(output) => {
            assert!(output.applied);
            assert!(!output.preview);
            assert_eq!(output.deleted_count, 4);
            assert_eq!(output.bytes_remaining, 0);
        }
        other => panic!("unexpected delete output: {other:?}"),
    }
    assert!(!repo_root.join("target").exists());
}

#[test]
fn tool_runtime_rename_previews_and_requires_guarded_overwrite() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let repo_root = tempdir.path();
    fs::write(repo_root.join("source.txt"), "source\n").expect("seed source");
    fs::write(repo_root.join("target.txt"), "target\n").expect("seed target");
    let runtime = AutonomousToolRuntime::new(repo_root).expect("runtime");

    let source_hash = match runtime
        .hash(AutonomousHashRequest {
            path: "source.txt".into(),
            recursive: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_files: None,
            manifest: false,
        })
        .expect("hash source")
        .output
    {
        AutonomousToolOutput::Hash(output) => output.sha256,
        other => panic!("unexpected source hash output: {other:?}"),
    };
    let target_hash = match runtime
        .hash(AutonomousHashRequest {
            path: "target.txt".into(),
            recursive: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_files: None,
            manifest: false,
        })
        .expect("hash target")
        .output
    {
        AutonomousToolOutput::Hash(output) => output.sha256,
        other => panic!("unexpected target hash output: {other:?}"),
    };

    let preview = runtime
        .rename(AutonomousRenameRequest {
            from_path: "source.txt".into(),
            to_path: "renamed.txt".into(),
            expected_hash: Some(source_hash.clone()),
            expected_target_hash: None,
            overwrite: None,
            preview: true,
        })
        .expect("preview rename");
    match preview.output {
        AutonomousToolOutput::Rename(output) => {
            assert!(output.preview);
            assert!(!output.applied);
            assert!(!output.overwritten);
            assert_eq!(output.source_kind, AutonomousStatKind::File);
            assert_eq!(output.source_bytes, Some(7));
            assert_eq!(output.source_hash.as_deref(), Some(source_hash.as_str()));
            assert!(!output.target_existed);
        }
        other => panic!("unexpected rename preview output: {other:?}"),
    }
    assert!(repo_root.join("source.txt").exists());
    assert!(!repo_root.join("renamed.txt").exists());

    let target_exists = runtime
        .rename(AutonomousRenameRequest {
            from_path: "source.txt".into(),
            to_path: "target.txt".into(),
            expected_hash: Some(source_hash.clone()),
            expected_target_hash: None,
            overwrite: None,
            preview: false,
        })
        .expect_err("existing target requires overwrite guard");
    assert_eq!(target_exists.code, "autonomous_tool_rename_target_exists");

    let missing_target_hash = runtime
        .rename(AutonomousRenameRequest {
            from_path: "source.txt".into(),
            to_path: "target.txt".into(),
            expected_hash: Some(source_hash.clone()),
            expected_target_hash: None,
            overwrite: Some(true),
            preview: false,
        })
        .expect_err("overwrite requires target hash");
    assert_eq!(
        missing_target_hash.code,
        "autonomous_tool_rename_expected_target_hash_required"
    );

    let overwrite_preview = runtime
        .rename(AutonomousRenameRequest {
            from_path: "source.txt".into(),
            to_path: "target.txt".into(),
            expected_hash: Some(source_hash.clone()),
            expected_target_hash: Some(target_hash.clone()),
            overwrite: Some(true),
            preview: true,
        })
        .expect("preview guarded overwrite");
    match overwrite_preview.output {
        AutonomousToolOutput::Rename(output) => {
            assert!(output.preview);
            assert!(output.overwritten);
            assert!(output.target_existed);
            assert_eq!(output.target_kind, Some(AutonomousStatKind::File));
            assert_eq!(output.target_bytes, Some(7));
            assert_eq!(output.target_hash.as_deref(), Some(target_hash.as_str()));
        }
        other => panic!("unexpected guarded rename preview output: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(repo_root.join("target.txt")).expect("target remains after preview"),
        "target\n"
    );

    let applied = runtime
        .rename(AutonomousRenameRequest {
            from_path: "source.txt".into(),
            to_path: "target.txt".into(),
            expected_hash: Some(source_hash),
            expected_target_hash: Some(target_hash),
            overwrite: Some(true),
            preview: false,
        })
        .expect("apply guarded overwrite rename");
    match applied.output {
        AutonomousToolOutput::Rename(output) => {
            assert!(output.applied);
            assert!(!output.preview);
            assert!(output.overwritten);
        }
        other => panic!("unexpected guarded rename output: {other:?}"),
    }
    assert!(!repo_root.join("source.txt").exists());
    assert_eq!(
        fs::read_to_string(repo_root.join("target.txt")).expect("target replaced"),
        "source\n"
    );
}

#[test]
fn tool_runtime_mkdir_previews_parent_and_existence_modes() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let repo_root = tempdir.path();
    fs::create_dir_all(repo_root.join("existing")).expect("existing dir");
    let runtime = AutonomousToolRuntime::new(repo_root).expect("runtime");

    let preview = runtime
        .mkdir(AutonomousMkdirRequest {
            path: "a/b/c".into(),
            parents: Some(true),
            exist_ok: Some(true),
            preview: true,
        })
        .expect("preview mkdir parents");
    match preview.output {
        AutonomousToolOutput::Mkdir(output) => {
            assert!(output.created);
            assert!(output.preview);
            assert!(!output.applied);
            assert!(output.parents);
            assert!(output.exist_ok);
            assert_eq!(output.created_paths, vec!["a", "a/b", "a/b/c"]);
        }
        other => panic!("unexpected mkdir preview output: {other:?}"),
    }
    assert!(!repo_root.join("a").exists());

    let no_parents = runtime
        .mkdir(AutonomousMkdirRequest {
            path: "a/b/c".into(),
            parents: Some(false),
            exist_ok: Some(true),
            preview: false,
        })
        .expect_err("parents=false refuses missing parent chain");
    assert_eq!(no_parents.code, "autonomous_tool_mkdir_parent_missing");

    let applied = runtime
        .mkdir(AutonomousMkdirRequest {
            path: "a/b/c".into(),
            parents: Some(true),
            exist_ok: Some(true),
            preview: false,
        })
        .expect("apply mkdir parents");
    match applied.output {
        AutonomousToolOutput::Mkdir(output) => {
            assert!(output.created);
            assert!(output.applied);
            assert!(!output.preview);
            assert_eq!(output.created_paths, vec!["a", "a/b", "a/b/c"]);
        }
        other => panic!("unexpected mkdir output: {other:?}"),
    }
    assert!(repo_root.join("a/b/c").is_dir());

    let exists = runtime
        .mkdir(AutonomousMkdirRequest {
            path: "existing".into(),
            parents: Some(true),
            exist_ok: Some(false),
            preview: false,
        })
        .expect_err("existOk=false refuses existing directory");
    assert_eq!(exists.code, "autonomous_tool_mkdir_exists");
}

#[test]
fn tool_runtime_copy_previews_and_requires_overwrite_or_directory_digest() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let repo_root = tempdir.path();
    fs::write(repo_root.join("source.txt"), "source\n").expect("seed source");
    fs::write(repo_root.join("target.txt"), "target\n").expect("seed target");
    fs::create_dir_all(repo_root.join("srcdir/nested")).expect("seed source dir");
    fs::write(repo_root.join("srcdir/nested/a.txt"), "alpha\n").expect("seed nested file");
    let runtime = AutonomousToolRuntime::new(repo_root).expect("runtime");

    let source_hash = match runtime
        .hash(AutonomousHashRequest {
            path: "source.txt".into(),
            recursive: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_files: None,
            manifest: false,
        })
        .expect("hash source")
        .output
    {
        AutonomousToolOutput::Hash(output) => output.sha256,
        other => panic!("unexpected source hash output: {other:?}"),
    };
    let target_hash = match runtime
        .hash(AutonomousHashRequest {
            path: "target.txt".into(),
            recursive: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_files: None,
            manifest: false,
        })
        .expect("hash target")
        .output
    {
        AutonomousToolOutput::Hash(output) => output.sha256,
        other => panic!("unexpected target hash output: {other:?}"),
    };

    let implicit_overwrite = runtime
        .copy(AutonomousCopyRequest {
            from: "source.txt".into(),
            to: "target.txt".into(),
            recursive: false,
            expected_source_hash: Some(source_hash.clone()),
            expected_source_digest: None,
            overwrite: None,
            expected_target_hash: None,
            preview: false,
        })
        .expect_err("copy refuses implicit overwrite");
    assert_eq!(
        implicit_overwrite.code,
        "autonomous_tool_copy_target_exists"
    );

    let file_preview = runtime
        .copy(AutonomousCopyRequest {
            from: "source.txt".into(),
            to: "target.txt".into(),
            recursive: false,
            expected_source_hash: Some(source_hash.clone()),
            expected_source_digest: None,
            overwrite: Some(true),
            expected_target_hash: Some(target_hash.clone()),
            preview: true,
        })
        .expect("preview guarded file copy");
    match file_preview.output {
        AutonomousToolOutput::Copy(output) => {
            assert!(output.preview);
            assert!(!output.applied);
            assert!(output.overwritten);
            assert_eq!(output.copied_files, 1);
            assert_eq!(output.copied_bytes, 7);
            assert_eq!(output.source_hash.as_deref(), Some(source_hash.as_str()));
            assert_eq!(output.target_hash.as_deref(), Some(target_hash.as_str()));
        }
        other => panic!("unexpected copy file preview output: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(repo_root.join("target.txt")).expect("target unchanged after preview"),
        "target\n"
    );

    let dir_preview = runtime
        .copy(AutonomousCopyRequest {
            from: "srcdir".into(),
            to: "dstdir".into(),
            recursive: true,
            expected_source_hash: None,
            expected_source_digest: None,
            overwrite: None,
            expected_target_hash: None,
            preview: true,
        })
        .expect("preview directory copy");
    let source_digest = match dir_preview.output {
        AutonomousToolOutput::Copy(output) => {
            assert_eq!(output.source_kind, AutonomousStatKind::Directory);
            assert!(output.preview);
            assert_eq!(output.created_directories, 2);
            assert_eq!(output.copied_files, 1);
            assert_eq!(output.copied_bytes, 6);
            assert_eq!(output.operations.len(), 3);
            output.source_digest.expect("source digest")
        }
        other => panic!("unexpected copy directory preview output: {other:?}"),
    };
    assert_eq!(source_digest.len(), 64);
    assert!(!repo_root.join("dstdir").exists());

    let missing_digest = runtime
        .copy(AutonomousCopyRequest {
            from: "srcdir".into(),
            to: "dstdir".into(),
            recursive: true,
            expected_source_hash: None,
            expected_source_digest: None,
            overwrite: None,
            expected_target_hash: None,
            preview: false,
        })
        .expect_err("directory copy apply requires digest");
    assert_eq!(
        missing_digest.code,
        "autonomous_tool_copy_expected_source_digest_required"
    );

    let applied_dir = runtime
        .copy(AutonomousCopyRequest {
            from: "srcdir".into(),
            to: "dstdir".into(),
            recursive: true,
            expected_source_hash: None,
            expected_source_digest: Some(source_digest),
            overwrite: None,
            expected_target_hash: None,
            preview: false,
        })
        .expect("apply digest-guarded directory copy");
    match applied_dir.output {
        AutonomousToolOutput::Copy(output) => {
            assert!(output.applied);
            assert!(!output.preview);
            assert_eq!(output.created_directories, 2);
            assert_eq!(output.copied_files, 1);
        }
        other => panic!("unexpected copy directory output: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(repo_root.join("dstdir/nested/a.txt")).expect("copied file"),
        "alpha\n"
    );
}

#[test]
fn tool_runtime_fs_transaction_previews_applies_and_requires_directory_guards() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let repo_root = tempdir.path();
    fs::write(repo_root.join("existing.txt"), "old\n").expect("seed existing");
    fs::create_dir_all(repo_root.join("tree/nested")).expect("seed tree");
    fs::write(repo_root.join("tree/nested/a.txt"), "alpha\n").expect("seed tree file");
    let runtime = AutonomousToolRuntime::new(repo_root).expect("runtime");

    let existing_hash = match runtime
        .hash(AutonomousHashRequest {
            path: "existing.txt".into(),
            recursive: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_files: None,
            manifest: false,
        })
        .expect("hash existing")
        .output
    {
        AutonomousToolOutput::Hash(output) => output.sha256,
        other => panic!("unexpected hash output: {other:?}"),
    };

    let preview = runtime
        .fs_transaction(AutonomousFsTransactionRequest {
            preview: true,
            stop_on_first_error: false,
            operations: vec![
                AutonomousFsTransactionOperation {
                    id: Some("create".into()),
                    action: AutonomousFsTransactionAction::CreateFile,
                    path: Some("created.txt".into()),
                    content: Some("created\n".into()),
                    ..Default::default()
                },
                AutonomousFsTransactionOperation {
                    id: Some("replace".into()),
                    action: AutonomousFsTransactionAction::ReplaceFile,
                    path: Some("existing.txt".into()),
                    content: Some("new\n".into()),
                    expected_hash: Some(existing_hash.clone()),
                    ..Default::default()
                },
                AutonomousFsTransactionOperation {
                    id: Some("mkdir".into()),
                    action: AutonomousFsTransactionAction::Mkdir,
                    path: Some("a/b".into()),
                    parents: Some(true),
                    exist_ok: Some(true),
                    ..Default::default()
                },
            ],
        })
        .expect("preview transaction");
    match preview.output {
        AutonomousToolOutput::FsTransaction(output) => {
            assert!(output.preview);
            assert!(!output.applied);
            assert!(output.validation.ok);
            assert_eq!(output.operation_count, 3);
            assert!(output.changed_paths.contains(&"created.txt".into()));
            assert!(output.changed_paths.contains(&"existing.txt".into()));
            assert!(output.changed_paths.contains(&"a/b".into()));
            assert!(output.diff.as_deref().unwrap_or_default().contains("+new"));
        }
        other => panic!("unexpected fs_transaction preview output: {other:?}"),
    }
    assert!(!repo_root.join("created.txt").exists());
    assert_eq!(
        fs::read_to_string(repo_root.join("existing.txt")).expect("existing unchanged"),
        "old\n"
    );

    let applied = runtime
        .fs_transaction(AutonomousFsTransactionRequest {
            preview: false,
            stop_on_first_error: true,
            operations: vec![
                AutonomousFsTransactionOperation {
                    id: Some("create".into()),
                    action: AutonomousFsTransactionAction::CreateFile,
                    path: Some("created.txt".into()),
                    content: Some("created\n".into()),
                    ..Default::default()
                },
                AutonomousFsTransactionOperation {
                    id: Some("replace".into()),
                    action: AutonomousFsTransactionAction::ReplaceFile,
                    path: Some("existing.txt".into()),
                    content: Some("new\n".into()),
                    expected_hash: Some(existing_hash),
                    ..Default::default()
                },
            ],
        })
        .expect("apply transaction");
    match applied.output {
        AutonomousToolOutput::FsTransaction(output) => {
            assert!(output.applied);
            assert!(!output.preview);
            assert!(output.validation.ok);
            assert!(!output.rollback_status.attempted);
            assert_eq!(output.results.len(), 2);
            assert!(output.results.iter().all(|result| result.ok));
        }
        other => panic!("unexpected fs_transaction apply output: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(repo_root.join("created.txt")).expect("created file"),
        "created\n"
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("existing.txt")).expect("replaced file"),
        "new\n"
    );

    let dir_preview = runtime
        .fs_transaction(AutonomousFsTransactionRequest {
            preview: true,
            stop_on_first_error: false,
            operations: vec![AutonomousFsTransactionOperation {
                id: Some("copy-dir".into()),
                action: AutonomousFsTransactionAction::Copy,
                from: Some("tree".into()),
                to: Some("tree-copy".into()),
                recursive: true,
                ..Default::default()
            }],
        })
        .expect("preview directory copy transaction");
    let digest = match dir_preview.output {
        AutonomousToolOutput::FsTransaction(output) => {
            assert!(output.validation.ok);
            assert_eq!(output.planned_operations.len(), 1);
            assert!(output
                .planned_operations
                .first()
                .expect("planned copy")
                .changed_paths
                .contains(&"tree-copy".into()));
            output.planned_operations[0]
                .source_digest
                .clone()
                .expect("transaction copy source digest")
        }
        other => panic!("unexpected fs_transaction dir preview output: {other:?}"),
    };

    let missing_digest = runtime
        .fs_transaction(AutonomousFsTransactionRequest {
            preview: false,
            stop_on_first_error: true,
            operations: vec![AutonomousFsTransactionOperation {
                id: Some("copy-dir".into()),
                action: AutonomousFsTransactionAction::Copy,
                from: Some("tree".into()),
                to: Some("tree-copy".into()),
                recursive: true,
                ..Default::default()
            }],
        })
        .expect("transaction validation returns structured error");
    match missing_digest.output {
        AutonomousToolOutput::FsTransaction(output) => {
            assert!(!output.validation.ok);
            assert_eq!(output.validation.errors.len(), 1);
            assert_eq!(
                output.validation.errors[0]
                    .error
                    .as_ref()
                    .expect("validation error")
                    .code,
                "autonomous_tool_fs_transaction_expected_source_digest_required"
            );
        }
        other => panic!("unexpected missing digest output: {other:?}"),
    }

    let applied_copy = runtime
        .fs_transaction(AutonomousFsTransactionRequest {
            preview: false,
            stop_on_first_error: true,
            operations: vec![AutonomousFsTransactionOperation {
                id: Some("copy-dir".into()),
                action: AutonomousFsTransactionAction::Copy,
                from: Some("tree".into()),
                to: Some("tree-copy".into()),
                recursive: true,
                expected_source_digest: Some(digest),
                ..Default::default()
            }],
        })
        .expect("apply guarded directory copy transaction");
    match applied_copy.output {
        AutonomousToolOutput::FsTransaction(output) => {
            assert!(output.applied);
            assert!(output.validation.ok);
            assert!(output.changed_paths.contains(&"tree-copy".into()));
        }
        other => panic!("unexpected applied copy transaction output: {other:?}"),
    }
    assert_eq!(
        fs::read_to_string(repo_root.join("tree-copy/nested/a.txt")).expect("copied tree file"),
        "alpha\n"
    );
}

#[test]
fn tool_runtime_structured_edits_parse_preview_and_apply_json_toml_yaml() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let repo_root = tempdir.path();
    fs::write(
        repo_root.join("package.json"),
        "{\n  \"scripts\": {\"test\": \"old\"},\n  \"keywords\": [\"xero\"]\n}\n",
    )
    .expect("seed json");
    fs::write(repo_root.join("Cargo.toml"), "[package]\nname = \"old\"\n").expect("seed toml");
    fs::write(
        repo_root.join("workflow.yml"),
        "name: old\njobs:\n  test:\n    runs-on: ubuntu-latest\n",
    )
    .expect("seed yaml");
    let runtime = AutonomousToolRuntime::new(repo_root).expect("runtime");

    let json_hash = match runtime
        .hash(AutonomousHashRequest {
            path: "package.json".into(),
            recursive: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_files: None,
            manifest: false,
        })
        .expect("hash json")
        .output
    {
        AutonomousToolOutput::Hash(output) => output.sha256,
        other => panic!("unexpected hash output: {other:?}"),
    };

    let preview = runtime
        .structured_edit(
            AutonomousStructuredEditRequest {
                path: "package.json".into(),
                expected_hash: Some(json_hash.clone()),
                formatting_mode: Default::default(),
                preview: true,
                operations: vec![
                    AutonomousStructuredEditOperation {
                        action: AutonomousStructuredEditAction::Set,
                        pointer: "/scripts/test".into(),
                        value: Some(serde_json::json!("vitest")),
                    },
                    AutonomousStructuredEditOperation {
                        action: AutonomousStructuredEditAction::AppendUnique,
                        pointer: "/keywords".into(),
                        value: Some(serde_json::json!("agent")),
                    },
                    AutonomousStructuredEditOperation {
                        action: AutonomousStructuredEditAction::SortKeys,
                        pointer: "".into(),
                        value: None,
                    },
                ],
            },
            AutonomousStructuredEditFormat::Json,
            "json_edit",
        )
        .expect("preview json edit");
    match preview.output {
        AutonomousToolOutput::JsonEdit(output) => {
            assert!(output.preview);
            assert!(!output.applied);
            assert_eq!(output.operations_applied, 3);
            assert!(output
                .semantic_changes
                .contains(&"set /scripts/test".into()));
            assert!(output
                .diff
                .as_deref()
                .unwrap_or_default()
                .contains("+    \"test\": \"vitest\""));
        }
        other => panic!("unexpected json_edit preview output: {other:?}"),
    }
    assert!(fs::read_to_string(repo_root.join("package.json"))
        .expect("json unchanged")
        .contains("\"old\""));

    runtime
        .structured_edit(
            AutonomousStructuredEditRequest {
                path: "package.json".into(),
                expected_hash: Some(json_hash),
                formatting_mode: Default::default(),
                preview: false,
                operations: vec![AutonomousStructuredEditOperation {
                    action: AutonomousStructuredEditAction::Set,
                    pointer: "/scripts/test".into(),
                    value: Some(serde_json::json!("vitest")),
                }],
            },
            AutonomousStructuredEditFormat::Json,
            "json_edit",
        )
        .expect("apply json edit");
    assert!(fs::read_to_string(repo_root.join("package.json"))
        .expect("json applied")
        .contains("\"vitest\""));

    runtime
        .structured_edit(
            AutonomousStructuredEditRequest {
                path: "Cargo.toml".into(),
                expected_hash: None,
                formatting_mode: Default::default(),
                preview: false,
                operations: vec![AutonomousStructuredEditOperation {
                    action: AutonomousStructuredEditAction::Set,
                    pointer: "/package/name".into(),
                    value: Some(serde_json::json!("new-name")),
                }],
            },
            AutonomousStructuredEditFormat::Toml,
            "toml_edit",
        )
        .expect("apply toml edit");
    assert!(fs::read_to_string(repo_root.join("Cargo.toml"))
        .expect("toml applied")
        .contains("new-name"));

    runtime
        .structured_edit(
            AutonomousStructuredEditRequest {
                path: "workflow.yml".into(),
                expected_hash: None,
                formatting_mode: Default::default(),
                preview: false,
                operations: vec![AutonomousStructuredEditOperation {
                    action: AutonomousStructuredEditAction::Set,
                    pointer: "/name".into(),
                    value: Some(serde_json::json!("ci")),
                }],
            },
            AutonomousStructuredEditFormat::Yaml,
            "yaml_edit",
        )
        .expect("apply yaml edit");
    assert!(fs::read_to_string(repo_root.join("workflow.yml"))
        .expect("yaml applied")
        .contains("name: ci"));
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
    .expect("build autonomous tool runtime")
    .with_solana_executor(std::sync::Arc::new(
        xero_desktop_lib::runtime::autonomous_tool_runtime::UnavailableSolanaExecutor,
    ));

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
fn tool_runtime_read_many_returns_ordered_results_and_per_file_errors() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::write(
        repo_root.join("src").join("large.txt"),
        "0123456789abcdef\n",
    )
    .expect("seed oversized file");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let result = runtime
        .read_many(AutonomousReadManyRequest {
            paths: vec![
                "src/tracked.txt".into(),
                "src/missing.txt".into(),
                "src/large.txt".into(),
            ],
            mode: Some(AutonomousReadMode::Text),
            start_line: Some(1),
            line_count: Some(1),
            max_bytes_per_file: Some(8),
            max_total_bytes: Some(64),
            include_line_hashes: true,
        })
        .expect("read_many returns a batch result");

    match result.output {
        AutonomousToolOutput::ReadMany(output) => {
            assert_eq!(output.total_files, 3);
            assert_eq!(output.ok_files, 1);
            assert_eq!(output.error_files, 2);
            assert_eq!(output.omitted_files, 1);
            assert!(output.truncated);
            assert_eq!(output.results[0].path, "src/tracked.txt");
            assert!(output.results[0].ok);
            let read = output.results[0].read.as_ref().expect("read output");
            assert_eq!(read.content, "alpha\n");
            assert_eq!(read.line_hashes.len(), 1);
            assert_eq!(output.results[1].path, "src/missing.txt");
            assert!(!output.results[1].ok);
            assert_eq!(
                output.results[1]
                    .error
                    .as_ref()
                    .expect("missing error")
                    .code,
                "autonomous_tool_path_not_found"
            );
            assert_eq!(output.results[2].path, "src/large.txt");
            assert_eq!(
                output.results[2].error.as_ref().expect("large error").code,
                "autonomous_tool_read_many_file_too_large"
            );
            assert_eq!(output.results[2].omitted_bytes, Some(17));
        }
        other => panic!("unexpected read_many output: {other:?}"),
    }
}

#[test]
fn tool_runtime_list_tree_returns_stable_tree_and_omission_counts() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::create_dir_all(repo_root.join("src").join("nested")).expect("seed nested dir");
    fs::write(
        repo_root.join("src").join("nested").join("mod.rs"),
        "pub fn nested() {}\n",
    )
    .expect("seed nested file");
    fs::write(repo_root.join("src").join("skip.log"), "skip\n").expect("seed filtered file");
    fs::write(repo_root.join("src").join("tracked.txt"), "alpha\nbeta\n")
        .expect("modify tracked file");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let result = runtime
        .list_tree(AutonomousListTreeRequest {
            path: Some("src".into()),
            max_depth: Some(1),
            max_entries: Some(10),
            include_globs: Vec::new(),
            exclude_globs: vec!["**/*.log".into()],
            include_git_status: true,
            show_omitted: true,
        })
        .expect("list_tree succeeds");

    match result.output {
        AutonomousToolOutput::ListTree(output) => {
            assert_eq!(output.path, "src");
            assert_eq!(output.root.path, "src");
            assert_eq!(output.root.children.len(), 2);
            assert_eq!(output.root.children[0].path, "src/nested");
            assert_eq!(output.root.children[1].path, "src/tracked.txt");
            assert_eq!(output.file_count, 1);
            assert_eq!(output.directory_count, 2);
            assert_eq!(output.omitted.depth, 1);
            assert_eq!(output.omitted.filtered, 1);
            assert!(output.truncated);
            assert!(output
                .git_status
                .iter()
                .any(|entry| entry.path == "src/tracked.txt" && entry.unstaged.is_some()));
        }
        other => panic!("unexpected list_tree output: {other:?}"),
    }
}

#[test]
fn tool_runtime_directory_digest_is_deterministic_and_reports_omissions() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::create_dir_all(repo_root.join("src").join("nested")).expect("seed nested dir");
    fs::write(
        repo_root.join("src").join("nested").join("mod.rs"),
        "pub fn nested() {}\n",
    )
    .expect("seed nested file");
    fs::write(repo_root.join("src").join("skip.log"), "skip\n").expect("seed filtered file");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let first = runtime
        .directory_digest(AutonomousDirectoryDigestRequest {
            path: "src".into(),
            include_globs: vec!["**/*.rs".into()],
            exclude_globs: vec!["**/*.log".into()],
            max_files: Some(2),
            hash_mode: Some(AutonomousDirectoryDigestHashMode::ContentHash),
        })
        .expect("directory digest succeeds");
    let second = runtime
        .directory_digest(AutonomousDirectoryDigestRequest {
            path: "src".into(),
            include_globs: vec!["**/*.rs".into()],
            exclude_globs: vec!["**/*.log".into()],
            max_files: Some(2),
            hash_mode: Some(AutonomousDirectoryDigestHashMode::ContentHash),
        })
        .expect("directory digest is repeatable");

    match (first.output, second.output) {
        (
            AutonomousToolOutput::DirectoryDigest(first),
            AutonomousToolOutput::DirectoryDigest(second),
        ) => {
            assert_eq!(first.digest, second.digest);
            assert_eq!(
                first.hash_mode,
                AutonomousDirectoryDigestHashMode::ContentHash
            );
            assert_eq!(first.file_count, 1);
            assert_eq!(first.directory_count, 2);
            assert_eq!(first.omitted.filtered, 2);
            assert!(first.truncated);
            assert!(first
                .manifest
                .iter()
                .any(|entry| entry.path == "src/nested/mod.rs" && entry.sha256.is_some()));
        }
        other => panic!("unexpected directory_digest output: {other:?}"),
    }
}

#[test]
fn tool_runtime_file_hash_supports_file_sets_and_app_data_manifests() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::create_dir_all(repo_root.join("src").join("nested")).expect("seed nested dir");
    fs::write(repo_root.join("src").join("a.rs"), "pub fn a() {}\n").expect("seed a");
    fs::write(
        repo_root.join("src").join("nested").join("b.rs"),
        "pub fn b() {}\n",
    )
    .expect("seed b");
    fs::write(repo_root.join("src").join("skip.log"), "skip\n").expect("seed skip");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let single = runtime
        .hash(AutonomousHashRequest {
            path: "src/a.rs".into(),
            recursive: false,
            include_globs: Vec::new(),
            exclude_globs: Vec::new(),
            max_files: None,
            manifest: false,
        })
        .expect("single file hash");
    match single.output {
        AutonomousToolOutput::Hash(output) => {
            assert_eq!(output.path, "src/a.rs");
            assert_eq!(output.path_kind, AutonomousStatKind::File);
            assert_eq!(output.algorithm, "sha256");
            assert_eq!(output.mode, "single_file");
            assert_eq!(output.file_count, 1);
            assert_eq!(output.bytes, 14);
            assert_eq!(output.sha256.len(), 64);
            assert!(output.artifact_path.is_none());
        }
        other => panic!("unexpected single file hash output: {other:?}"),
    }

    let file_set = runtime
        .hash(AutonomousHashRequest {
            path: "src".into(),
            recursive: true,
            include_globs: vec!["**/*.rs".into()],
            exclude_globs: vec!["**/skip*".into()],
            max_files: Some(1),
            manifest: true,
        })
        .expect("file set hash");
    match file_set.output {
        AutonomousToolOutput::Hash(output) => {
            assert_eq!(output.path, "src");
            assert_eq!(output.path_kind, AutonomousStatKind::Directory);
            assert_eq!(output.mode, "file_set");
            assert_eq!(output.algorithm, "sha256");
            assert_eq!(output.file_count, 1);
            assert_eq!(output.max_files, 1);
            assert!(output.truncated);
            assert_eq!(output.omitted.max_files, 1);
            assert!(output.omitted.filtered >= 1);
            assert_eq!(output.files.len(), 1);
            assert!(output.files[0].path.ends_with(".rs"));
            let artifact_path = output.artifact_path.expect("manifest artifact path");
            let artifact_path = Path::new(&artifact_path);
            assert!(artifact_path.is_file());
            assert!(artifact_path.starts_with(db::project_app_data_dir_for_repo(&repo_root)));
            let manifest = fs::read_to_string(artifact_path).expect("read manifest artifact");
            assert!(manifest.contains("\"schemaVersion\": \"xero.file_hash_manifest.v1\""));
            assert!(manifest.contains("\"files\""));
        }
        other => panic!("unexpected file set hash output: {other:?}"),
    }
}

#[test]
fn tool_runtime_patch_persists_large_diff_artifacts_under_app_data() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let artifact_dir = repo_root.join("src").join("patch-artifacts");
    fs::create_dir_all(&artifact_dir).expect("seed artifact dir");
    let operations = (0..64)
        .map(|index| {
            let path =
                format!("src/patch-artifacts/very-long-file-name-for-artifact-diff-{index:02}.txt");
            fs::write(repo_root.join(&path), "old\n").expect("seed patch file");
            AutonomousPatchOperation {
                path,
                search: "old\n".into(),
                replace: "patched\n".into(),
                replace_all: false,
                expected_hash: None,
            }
        })
        .collect::<Vec<_>>();

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let result = runtime
        .patch(AutonomousPatchRequest {
            path: None,
            search: None,
            replace: None,
            replace_all: false,
            expected_hash: None,
            preview: false,
            operations,
        })
        .expect("large patch applies");

    match result.output {
        AutonomousToolOutput::Patch(output) => {
            assert!(output.applied);
            assert_eq!(output.files.len(), 64);
            assert_eq!(output.files[0].replacements, 1);
            assert!(output.files[0].guard_status.matched);
            assert_eq!(output.files[0].changed_ranges[0].start_line, 1);
            assert_eq!(output.files[0].changed_ranges[0].end_line, 1);
            assert!(!output.rollback_status.attempted);
            assert!(output.diff_truncated);
            let artifact_path = output.artifact_path.expect("patch diff artifact");
            let artifact_path = Path::new(&artifact_path);
            assert!(artifact_path.is_file());
            assert!(artifact_path.starts_with(db::project_app_data_dir_for_repo(&repo_root)));
            assert!(fs::read_to_string(artifact_path)
                .expect("read patch artifact")
                .contains("patched"));
        }
        other => panic!("unexpected patch output: {other:?}"),
    }
}

#[test]
fn tool_runtime_stat_reports_metadata_without_reading_content() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tracked_path = repo_root.join("src").join("tracked.txt");
    fs::write(&tracked_path, "alpha\nbeta\n").expect("modify tracked file");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let stat = runtime
        .stat(AutonomousStatRequest {
            path: "src/tracked.txt".into(),
            follow_symlinks: false,
            include_git_status: true,
            include_hash: true,
            strict: false,
        })
        .expect("stat tracked file");
    match stat.output {
        AutonomousToolOutput::Stat(output) => {
            assert_eq!(output.path, "src/tracked.txt");
            assert_eq!(output.path_kind, AutonomousStatKind::File);
            assert!(output.exists);
            assert_eq!(output.size, Some(11));
            assert_eq!(output.sha256.as_deref().map(str::len), Some(64));
            assert!(output.modified_at.is_some());
            assert!(output.permissions.is_some());
            assert!(output
                .git_status
                .iter()
                .any(|entry| entry.path == "src/tracked.txt" && entry.unstaged.is_some()));
        }
        other => panic!("unexpected stat output: {other:?}"),
    }

    let missing = runtime
        .stat(AutonomousStatRequest {
            path: "src/missing.txt".into(),
            follow_symlinks: false,
            include_git_status: true,
            include_hash: true,
            strict: false,
        })
        .expect("missing stat is successful observation");
    match missing.output {
        AutonomousToolOutput::Stat(output) => {
            assert_eq!(output.path_kind, AutonomousStatKind::Missing);
            assert!(!output.exists);
            assert!(output.sha256.is_none());
            assert!(output.git_status.is_empty());
        }
        other => panic!("unexpected missing stat output: {other:?}"),
    }

    let strict_missing = runtime
        .stat(AutonomousStatRequest {
            path: "src/missing.txt".into(),
            follow_symlinks: false,
            include_git_status: false,
            include_hash: false,
            strict: true,
        })
        .expect_err("strict missing stat should fail");
    assert_eq!(strict_missing.code, "autonomous_tool_stat_path_not_found");

    let directory = runtime
        .stat(AutonomousStatRequest {
            path: "src".into(),
            follow_symlinks: false,
            include_git_status: true,
            include_hash: true,
            strict: false,
        })
        .expect("stat directory");
    match directory.output {
        AutonomousToolOutput::Stat(output) => {
            assert_eq!(output.path_kind, AutonomousStatKind::Directory);
            assert!(output.size.is_none());
            assert!(output.sha256.is_none());
            assert!(output.hash_omitted_reason.is_some());
            assert!(output
                .git_status
                .iter()
                .any(|entry| entry.path == "src/tracked.txt"));
        }
        other => panic!("unexpected directory stat output: {other:?}"),
    }
}

#[cfg(unix)]
#[test]
fn tool_runtime_stat_reports_symlink_targets_without_following_by_default() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    std::os::unix::fs::symlink("src/tracked.txt", repo_root.join("tracked-link.txt"))
        .expect("create symlink");

    let runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let link = runtime
        .stat(AutonomousStatRequest {
            path: "tracked-link.txt".into(),
            follow_symlinks: false,
            include_git_status: false,
            include_hash: true,
            strict: false,
        })
        .expect("stat symlink");
    match link.output {
        AutonomousToolOutput::Stat(output) => {
            assert_eq!(output.path_kind, AutonomousStatKind::Symlink);
            assert_eq!(output.symlink_target.as_deref(), Some("src/tracked.txt"));
            assert!(output.sha256.is_none());
        }
        other => panic!("unexpected symlink stat output: {other:?}"),
    }

    let followed = runtime
        .stat(AutonomousStatRequest {
            path: "tracked-link.txt".into(),
            follow_symlinks: true,
            include_git_status: false,
            include_hash: true,
            strict: false,
        })
        .expect("stat followed symlink");
    match followed.output {
        AutonomousToolOutput::Stat(output) => {
            assert_eq!(output.path_kind, AutonomousStatKind::File);
            assert_eq!(output.resolved_path.as_deref(), Some("src/tracked.txt"));
            assert_eq!(output.sha256.as_deref().map(str::len), Some(64));
        }
        other => panic!("unexpected followed symlink stat output: {other:?}"),
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
            cursor: None,
            around_pattern: None,
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
            mode: None,
            path: Some("src".into()),
            max_depth: None,
            max_results: None,
            cursor: None,
        })
        .expect("zero-match find should still succeed");
    match empty_find.output {
        AutonomousToolOutput::Find(output) => assert!(output.matches.is_empty()),
        other => panic!("unexpected empty-find output: {other:?}"),
    }

    let invalid_find_pattern = runtime
        .find(AutonomousFindRequest {
            pattern: "[*.txt".into(),
            mode: None,
            path: None,
            max_depth: None,
            max_results: None,
            cursor: None,
        })
        .expect_err("malformed find patterns should be rejected");
    assert_eq!(
        invalid_find_pattern.code,
        "autonomous_tool_find_pattern_invalid"
    );

    let invalid_find_scope = runtime
        .find(AutonomousFindRequest {
            pattern: "**/*.txt".into(),
            mode: None,
            path: Some("../outside".into()),
            max_depth: None,
            max_results: None,
            cursor: None,
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
            preview: false,
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
            preview: false,
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
            preview: false,
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
            assert_eq!(
                output.policy.profile,
                AutonomousCommandPolicyProfile::ReadOnlyVerification
            );
            assert_eq!(output.policy.code, "policy_allowed_repo_scoped_command");
        }
        other => panic!("unexpected yolo command output: {other:?}"),
    }
}

#[test]
fn tool_runtime_command_persists_large_output_artifacts_and_metadata() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime =
        runtime_for_project_with_approval(&app, &project_id, RuntimeRunApprovalModeDto::Yolo);
    let lines = (0..260)
        .map(|index| format!("command artifact continuation line {index:03}"))
        .collect::<Vec<_>>();
    let print_steps = lines
        .iter()
        .map(|line| runtime_shell::script_print_line(line))
        .collect::<Vec<_>>();

    let result = runtime
        .command(AutonomousCommandRequest {
            argv: shell_argv(runtime_shell::script_join_steps(&print_steps)),
            cwd: None,
            timeout_ms: Some(5_000),
        })
        .expect("large command output succeeds");

    match result.output {
        AutonomousToolOutput::Command(output) => {
            assert_eq!(result.tool_name, "command");
            assert_eq!(output.intent, "general_execution");
            assert_eq!(output.exit_code, Some(0));
            assert!(output.stdout_truncated);
            let artifact = output.output_artifact.expect("command output artifact");
            let artifact_path = Path::new(&artifact.path);
            assert!(artifact_path.is_file());
            assert!(artifact_path.starts_with(db::project_app_data_dir_for_repo(&repo_root)));
            assert!(artifact.stdout_bytes > 2_000);
            assert!(artifact.truncated);
            assert!(!artifact.redacted);
            assert!(output
                .suggested_next_actions
                .iter()
                .any(|action| action.contains("outputArtifact.path")));
            let artifact_json: serde_json::Value =
                serde_json::from_str(&fs::read_to_string(artifact_path).expect("read artifact"))
                    .expect("command artifact json");
            assert_eq!(artifact_json["schema"], "xero.command_output_artifact.v1");
            assert!(artifact_json["stdout"]
                .as_str()
                .expect("artifact stdout")
                .contains("command artifact continuation line 120"));
        }
        other => panic!("unexpected command output: {other:?}"),
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
            assert_eq!(
                output.policy.profile,
                AutonomousCommandPolicyProfile::DestructiveOperation
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
            assert_eq!(
                output.policy.profile,
                AutonomousCommandPolicyProfile::ExternalNetwork
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
                output.policy.profile,
                AutonomousCommandPolicyProfile::DependencyInstallation
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
            mode: None,
            path: Some("fixtures".into()),
            max_depth: None,
            max_results: None,
            cursor: None,
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
            cursor: None,
            around_pattern: None,
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
            expected_hash: None,
            create_only: false,
            overwrite: None,
            preview: false,
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
            mode: None,
            path: None,
            max_depth: None,
            max_results: None,
            cursor: None,
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
            cursor: None,
            around_pattern: None,
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
