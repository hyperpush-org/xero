use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use git2::{IndexAddOption, Repository, Signature};
use rusqlite::{params, Connection};
use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;
use xero_desktop_lib::{
    commands::{
        archive_agent_session, cancel_agent_run, compact_session_history, start_agent_task,
        start_runtime_run, update_runtime_run_controls, ArchiveAgentSessionRequestDto,
        BrowserControlPreferenceDto, CancelAgentRunRequestDto, CompactSessionHistoryRequestDto,
        RuntimeAgentIdDto, RuntimeRunActiveControlSnapshotDto, RuntimeRunApprovalModeDto,
        RuntimeRunControlInputDto, RuntimeRunControlStateDto, StartAgentTaskRequestDto,
        StartRuntimeRunRequestDto, UpdateRuntimeRunControlsRequestDto,
    },
    configure_builder_with_state, db,
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    runtime::{
        continue_owned_agent_run, create_owned_agent_run, run_owned_agent_task,
        AgentAutoCompactPreference, AgentProviderConfig, AgentToolCall, AutonomousCommandRequest,
        AutonomousCommandSessionOperation, AutonomousCommandSessionStartRequest,
        AutonomousCommandSessionStopRequest, AutonomousToolOutput, AutonomousToolRuntime,
        ContinueOwnedAgentRunRequest, OpenAiCompatibleProviderConfig, OwnedAgentRunRequest,
        ToolRegistry, ToolRegistryOptions,
    },
    state::DesktopState,
};

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    DesktopState::default()
        .with_global_db_path_override(root.path().join("app-data").join("xero.db"))
        .with_owned_agent_provider_config_override(AgentProviderConfig::Fake)
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> (String, PathBuf) {
    let repo_root = root.path().join("repo");
    fs::create_dir_all(repo_root.join("src")).expect("create repo src");
    fs::write(repo_root.join("src").join("tracked.txt"), "alpha\nbeta\n")
        .expect("seed tracked file");
    fs::write(
        repo_root.join("AGENTS.md"),
        "- Keep test work repo-local.\n- Prefer direct tool evidence.\n",
    )
    .expect("seed repo instructions");

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

    let desktop_state = app.state::<DesktopState>();
    let global_db_path = desktop_state
        .global_db_path(&app.handle().clone())
        .expect("global db path");
    db::configure_project_database_paths(&global_db_path);

    db::import_project(&repository, desktop_state.import_failpoints())
        .expect("import project into app-data db");

    let registry_path = app
        .state::<DesktopState>()
        .global_db_path(&app.handle().clone())
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

fn yolo_controls() -> RuntimeRunControlStateDto {
    RuntimeRunControlStateDto {
        active: RuntimeRunActiveControlSnapshotDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            provider_profile_id: None,
            model_id: "test-model".into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Yolo,
            plan_mode_required: false,
            revision: 1,
            applied_at: "2026-04-24T00:00:00Z".into(),
        },
        pending: None,
    }
}

fn yolo_controls_input() -> RuntimeRunControlInputDto {
    RuntimeRunControlInputDto {
        runtime_agent_id: RuntimeAgentIdDto::Engineer,
        agent_definition_id: None,
        provider_profile_id: None,
        model_id: "test-model".into(),
        thinking_effort: None,
        approval_mode: RuntimeRunApprovalModeDto::Yolo,
        plan_mode_required: false,
    }
}

fn suggest_controls_input() -> RuntimeRunControlInputDto {
    RuntimeRunControlInputDto {
        runtime_agent_id: RuntimeAgentIdDto::Engineer,
        agent_definition_id: None,
        provider_profile_id: None,
        model_id: "test-model".into(),
        thinking_effort: None,
        approval_mode: RuntimeRunApprovalModeDto::Suggest,
        plan_mode_required: false,
    }
}

fn wait_for_agent_run_status(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    status: db::project_store::AgentRunStatus,
) -> db::project_store::AgentRunSnapshotRecord {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match db::project_store::load_agent_run(repo_root, project_id, run_id) {
            Ok(snapshot) if snapshot.run.status == status => return snapshot,
            Ok(snapshot) => {
                assert!(
                    Instant::now() < deadline,
                    "owned agent run {run_id} did not reach {status:?}; last status was {:?}",
                    snapshot.run.status
                );
            }
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "owned agent run {run_id} was not persisted while waiting for {status:?}: {error:?}"
                );
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_agent_run_inactive(state: &DesktopState, run_id: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let active = state
            .agent_run_supervisor()
            .is_active(run_id)
            .expect("check owned-agent supervisor activity");
        if !active {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "owned agent run {run_id} remained active after reaching a terminal status"
        );
        thread::sleep(Duration::from_millis(25));
    }
}

fn wait_for_runtime_run_status(
    repo_root: &Path,
    project_id: &str,
    agent_session_id: &str,
    status: db::project_store::RuntimeRunStatus,
) -> db::project_store::RuntimeRunSnapshotRecord {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match db::project_store::load_runtime_run(repo_root, project_id, agent_session_id) {
            Ok(Some(snapshot)) if snapshot.run.status == status => return snapshot,
            Ok(Some(snapshot)) => {
                assert!(
                    Instant::now() < deadline,
                    "runtime run for session {agent_session_id} did not reach {status:?}; last status was {:?}",
                    snapshot.run.status
                );
            }
            Ok(None) => {
                assert!(
                    Instant::now() < deadline,
                    "runtime run for session {agent_session_id} was missing while waiting for {status:?}"
                );
            }
            Err(error) => {
                assert!(
                    Instant::now() < deadline,
                    "runtime run for session {agent_session_id} could not be loaded while waiting for {status:?}: {error:?}"
                );
            }
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn append_auto_compact_fixture_messages(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    timestamp_minute: &str,
    count: usize,
    chars_per_message: usize,
) {
    for index in 0..count {
        let role = if index % 2 == 0 {
            db::project_store::AgentMessageRole::Assistant
        } else {
            db::project_store::AgentMessageRole::User
        };
        db::project_store::append_agent_message(
            repo_root,
            &db::project_store::NewAgentMessageRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                role,
                content: format!(
                    "auto compact fixture message {index}: {}",
                    "x".repeat(chars_per_message)
                ),
                created_at: format!("{timestamp_minute}:{:02}Z", index + 3),
                attachments: Vec::new(),
            },
        )
        .expect("append auto compact fixture message");
    }
}

#[test]
fn owned_agent_tool_registry_exposes_provider_ready_schemas() {
    let registry = ToolRegistry::builtin_with_options(ToolRegistryOptions {
        runtime_agent_id: RuntimeAgentIdDto::Engineer,
        ..ToolRegistryOptions::default()
    });
    let descriptor_names = registry
        .descriptors()
        .iter()
        .map(|descriptor| descriptor.name.as_str())
        .collect::<BTreeSet<_>>();
    let expected_names = BTreeSet::from([
        "read",
        "search",
        "find",
        "git_status",
        "git_diff",
        "tool_access",
        "project_context",
        "edit",
        "write",
        "patch",
        "delete",
        "rename",
        "mkdir",
        "list",
        "file_hash",
        "command",
        "command_session_start",
        "command_session_read",
        "command_session_stop",
        "process_manager",
        "macos_automation",
        "mcp",
        "subagent",
        "todo",
        "notebook_edit",
        "code_intel",
        "lsp",
        "powershell",
        "tool_search",
        "web_search",
        "web_fetch",
        "browser",
        "emulator",
        "environment_context",
        "system_diagnostics",
        "solana_cluster",
        "solana_logs",
        "solana_tx",
        "solana_simulate",
        "solana_explain",
        "solana_alt",
        "solana_idl",
        "solana_codama",
        "solana_pda",
        "solana_program",
        "solana_deploy",
        "solana_upgrade_check",
        "solana_squads",
        "solana_verified_build",
        "solana_audit_static",
        "solana_audit_external",
        "solana_audit_fuzz",
        "solana_audit_coverage",
        "solana_replay",
        "solana_indexer",
        "solana_secrets",
        "solana_cluster_drift",
        "solana_cost",
        "solana_docs",
    ]);
    assert_eq!(descriptor_names, expected_names);

    let read = registry.descriptor("read").expect("read descriptor");
    assert_eq!(read.input_schema["type"], "object");
    assert_eq!(read.input_schema["additionalProperties"], false);
    assert_eq!(read.input_schema["required"], json!(["path"]));
    assert_eq!(
        read.input_schema["properties"]["startLine"]["type"],
        "integer"
    );

    let git_diff = registry
        .descriptor("git_diff")
        .expect("git diff descriptor");
    assert_eq!(
        git_diff.input_schema["properties"]["scope"]["enum"],
        json!(["staged", "unstaged", "worktree"])
    );
    let tool_access = registry
        .descriptor("tool_access")
        .expect("tool access descriptor");
    assert_eq!(tool_access.input_schema["required"], json!(["action"]));
    assert!(
        tool_access.input_schema["properties"]["groups"]["description"]
            .as_str()
            .expect("tool access groups description")
            .contains("process_manager")
    );
    let project_context = registry
        .descriptor("project_context")
        .expect("project context descriptor");
    assert_eq!(project_context.input_schema["required"], json!(["action"]));
    assert!(project_context.input_schema["properties"]["action"]["enum"]
        .as_array()
        .expect("project context action enum")
        .contains(&json!("search_project_records")));
    assert!(project_context.input_schema["properties"]["action"]["enum"]
        .as_array()
        .expect("project context action enum")
        .contains(&json!("propose_record_candidate")));

    let process_manager = registry
        .descriptor("process_manager")
        .expect("process manager descriptor");
    assert_eq!(process_manager.input_schema["required"], json!(["action"]));
    assert!(process_manager.input_schema["properties"]["action"]["enum"]
        .as_array()
        .expect("process manager action enum")
        .contains(&json!("async_start")));
    assert!(process_manager.description.contains("phase 5"));

    let macos = registry
        .descriptor("macos_automation")
        .expect("macos automation descriptor");
    assert_eq!(macos.input_schema["required"], json!(["action"]));
    assert!(macos.input_schema["properties"]["action"]["enum"]
        .as_array()
        .expect("macos action enum")
        .contains(&json!("mac_permissions")));
    assert!(macos.input_schema["properties"]["action"]["enum"]
        .as_array()
        .expect("macos action enum")
        .contains(&json!("mac_screenshot")));

    assert!(registry.descriptor("browser").is_some());
    assert!(registry.descriptor("mcp").is_some());
    assert!(registry.descriptor("subagent").is_some());
    assert!(registry.descriptor("todo").is_some());
    assert!(registry.descriptor("notebook_edit").is_some());
    assert!(registry.descriptor("code_intel").is_some());
    assert!(registry.descriptor("lsp").is_some());
    assert!(registry.descriptor("powershell").is_some());
    assert!(registry.descriptor("tool_search").is_some());
    let emulator = registry
        .descriptor("emulator")
        .expect("emulator descriptor");
    assert_eq!(emulator.input_schema["required"], json!(["action"]));
    assert!(emulator.input_schema["properties"]["action"]["enum"]
        .as_array()
        .expect("emulator action enum")
        .contains(&json!("launch_app")));
    assert!(emulator.input_schema["properties"]["action"]["enum"]
        .as_array()
        .expect("emulator action enum")
        .contains(&json!("screenshot")));
    assert!(registry.descriptor("solana_cluster").is_some());
    assert!(registry.descriptor("patch").is_some());
    assert!(registry.descriptor("delete").is_some());
    assert!(registry.descriptor("rename").is_some());
    assert!(registry.descriptor("mkdir").is_some());
    assert!(registry.descriptor("list").is_some());
    assert!(registry.descriptor("file_hash").is_some());

    registry
        .validate_call(&AgentToolCall {
            tool_call_id: "tool-call-valid-read".into(),
            tool_name: "read".into(),
            input: json!({ "path": "src/tracked.txt", "startLine": 1, "lineCount": 40 }),
        })
        .expect("valid read call should decode");

    let unknown = registry
        .validate_call(&AgentToolCall {
            tool_call_id: "tool-call-unknown".into(),
            tool_name: "unknown_tool".into(),
            input: json!({}),
        })
        .expect_err("unknown tools should be rejected");
    assert_eq!(unknown.code, "agent_tool_call_unknown");
}

#[test]
fn owned_agent_tool_registry_selects_contextual_toolsets() {
    let temp = TempDir::new().unwrap();
    let controls = yolo_controls();

    let read_only = ToolRegistry::for_prompt(
        temp.path(),
        "What is left to do in this harness?",
        &controls,
    );
    let read_only_names = read_only.descriptor_names();
    assert!(read_only_names.contains("read"));
    assert!(read_only_names.contains("tool_access"));
    assert!(read_only_names.contains("project_context"));
    assert!(read_only_names.contains("tool_search"));
    assert!(read_only_names.contains("todo"));
    assert!(read_only_names.contains("git_diff"));
    assert!(!read_only_names.contains("write"));
    assert!(!read_only_names.contains("command"));
    assert!(!read_only_names.contains("emulator"));
    assert!(!read_only_names.contains("solana_cluster"));

    let implementation = ToolRegistry::for_prompt(
        temp.path(),
        "Continue implementing the missing production-ready agent features and run tests.",
        &controls,
    );
    let implementation_names = implementation.descriptor_names();
    assert!(implementation_names.contains("write"));
    assert!(implementation_names.contains("patch"));
    assert!(implementation_names.contains("command"));
    assert!(implementation_names.contains("command_session_start"));
    assert!(!implementation_names.contains("emulator"));

    let process_manager = ToolRegistry::for_prompt(
        temp.path(),
        "Design the process manager for long-running process visibility.",
        &controls,
    );
    assert!(process_manager
        .descriptor_names()
        .contains("process_manager"));

    let macos = ToolRegistry::for_prompt(
        temp.path(),
        "Implement phase 7 macOS app/system automation with app list and screenshots.",
        &controls,
    );
    assert!(macos.descriptor_names().contains("macos_automation"));

    let audit = ToolRegistry::for_prompt(
        temp.path(),
        "Thoroughly audit the harness and verify it is production grade.",
        &controls,
    );
    let audit_names = audit.descriptor_names();
    assert!(audit_names.contains("read"));
    assert!(audit_names.contains("git_diff"));
    assert!(audit_names.contains("command"));

    fs::write(temp.path().join("Anchor.toml"), "[programs.localnet]\n")
        .expect("seed solana-looking workspace");
    let broad_solana_workspace = ToolRegistry::for_prompt(
        temp.path(),
        "What is left to do in this harness?",
        &controls,
    );
    assert!(!broad_solana_workspace
        .descriptor_names()
        .contains("solana_cluster"));

    let priority_tools = ToolRegistry::for_prompt(
        temp.path(),
        "Use MCP, subagents, todos, code intelligence, notebooks, and PowerShell.",
        &controls,
    );
    let priority_names = priority_tools.descriptor_names();
    assert!(priority_names.contains("mcp"));
    assert!(priority_names.contains("subagent"));
    assert!(priority_names.contains("todo"));
    assert!(priority_names.contains("tool_search"));
    assert!(priority_names.contains("code_intel"));
    assert!(priority_names.contains("lsp"));
    assert!(priority_names.contains("notebook_edit"));
    assert!(priority_names.contains("powershell"));

    let app_use = ToolRegistry::for_prompt(
        temp.path(),
        "Implement app use for Android emulator automation.",
        &controls,
    );
    assert!(app_use.descriptor_names().contains("emulator"));

    let solana = ToolRegistry::for_prompt(
        temp.path(),
        "Audit the Solana Anchor program and PDA handling.",
        &controls,
    );
    assert!(solana.descriptor_names().contains("solana_cluster"));

    let default_browser = ToolRegistry::for_prompt_with_options(
        temp.path(),
        "Open localhost in a browser and inspect the UI.",
        &controls,
        ToolRegistryOptions {
            browser_control_preference: BrowserControlPreferenceDto::Default,
            ..ToolRegistryOptions::default()
        },
    );
    let default_browser_names = default_browser.descriptor_names();
    assert!(default_browser_names.contains("browser"));
    assert!(default_browser_names.contains("macos_automation"));

    let in_app_browser = ToolRegistry::for_prompt_with_options(
        temp.path(),
        "Open localhost in a browser and inspect the UI.",
        &controls,
        ToolRegistryOptions {
            browser_control_preference: BrowserControlPreferenceDto::InAppBrowser,
            ..ToolRegistryOptions::default()
        },
    );
    let in_app_browser_names = in_app_browser.descriptor_names();
    assert!(in_app_browser_names.contains("browser"));
    assert!(!in_app_browser_names.contains("macos_automation"));

    let native_browser = ToolRegistry::for_prompt_with_options(
        temp.path(),
        "Open localhost in a browser and inspect the UI.",
        &controls,
        ToolRegistryOptions {
            browser_control_preference: BrowserControlPreferenceDto::NativeBrowser,
            ..ToolRegistryOptions::default()
        },
    );
    let native_browser_names = native_browser.descriptor_names();
    assert!(!native_browser_names.contains("browser"));
    assert!(native_browser_names.contains("macos_automation"));
}

#[test]
fn owned_agent_file_tools_cover_patch_hash_mkdir_rename_and_delete() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-file-tools-1".into(),
        prompt: [
            "Exercise file tools.",
            "tool:read src/tracked.txt",
            "tool:patch src/tracked.txt alpha ALPHA",
            "tool:hash src/tracked.txt",
            "tool:mkdir generated",
            "tool:write generated/new.txt hello",
            "tool:rename generated/new.txt generated/renamed.txt",
            "tool:delete generated/renamed.txt",
            "tool:command_echo verified-file-tools",
        ]
        .join("\n"),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent file tools should succeed");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed,
        "last error: {:?}",
        snapshot.run.last_error
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("src").join("tracked.txt"))
            .expect("patched tracked file"),
        "ALPHA\nbeta\n"
    );
    assert!(!repo_root.join("generated").join("renamed.txt").exists());

    let tool_names = snapshot
        .tool_calls
        .iter()
        .map(|tool_call| tool_call.tool_name.as_str())
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"patch"));
    assert!(tool_names.contains(&"command"));
    assert!(tool_names.contains(&"file_hash"));
    assert!(tool_names.contains(&"mkdir"));
    assert!(tool_names.contains(&"rename"));
    assert!(tool_names.contains(&"delete"));

    let operations = snapshot
        .file_changes
        .iter()
        .map(|change| change.operation.as_str())
        .collect::<Vec<_>>();
    assert!(operations.contains(&"patch"));
    assert!(operations.contains(&"mkdir"));
    assert!(operations.contains(&"create"));
    assert!(operations.contains(&"rename"));
    assert!(operations.contains(&"delete"));
}

#[test]
fn owned_agent_priority_one_tools_dispatch_and_persist_journal() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::write(
        repo_root.join("src").join("lib.rs"),
        "pub struct Greeter;\n\npub fn greet() {}\n",
    )
    .expect("seed rust source");
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-priority-tools-1".into(),
        prompt: [
            "Exercise priority one tools.",
            "tool:tool_search mcp",
            "tool:todo_upsert Inspect the owned-agent priority surface",
            "tool:subagent Explore priority one work",
            "tool:code_intel_symbols src/lib.rs greet",
            "tool:lsp_symbols src/lib.rs greet",
            "tool:mcp_list",
        ]
        .join("\n"),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent priority tools should succeed");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed,
        "last error: {:?}",
        snapshot.run.last_error
    );
    let tool_names = snapshot
        .tool_calls
        .iter()
        .map(|tool_call| tool_call.tool_name.as_str())
        .collect::<Vec<_>>();
    assert!(tool_names.contains(&"tool_search"));
    assert!(tool_names.contains(&"todo"));
    assert!(tool_names.contains(&"subagent"));
    assert!(tool_names.contains(&"code_intel"));
    assert!(tool_names.contains(&"lsp"));
    assert!(tool_names.contains(&"mcp"));
    assert!(snapshot.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::Tool
            && message.content.contains("\"toolName\":\"code_intel\"")
            && message.content.contains("\"name\":\"greet\"")
    }));
    assert!(snapshot.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::Tool
            && message.content.contains("\"toolName\":\"lsp\"")
            && message.content.contains("\"name\":\"greet\"")
    }));
    assert!(snapshot.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::Tool
            && message.content.contains("\"toolName\":\"subagent\"")
            && message.content.contains("\"status\":\"running\"")
            && message
                .content
                .contains("\"runId\":\"owned-run-priority-tools-1-subagent-1\"")
    }));
    let child_snapshot = wait_for_agent_run_status(
        &repo_root,
        &project_id,
        "owned-run-priority-tools-1-subagent-1",
        db::project_store::AgentRunStatus::Completed,
    );
    assert_eq!(
        child_snapshot.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert!(child_snapshot
        .run
        .prompt
        .contains("Explore priority one work"));
}

#[test]
fn owned_agent_loop_dispatches_tools_and_persists_journal() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    db::project_store::insert_agent_memory(
        &repo_root,
        &db::project_store::NewAgentMemoryRecord {
            memory_id: "memory-runtime-approved".into(),
            project_id: project_id.clone(),
            agent_session_id: None,
            scope: db::project_store::AgentMemoryScope::Project,
            kind: db::project_store::AgentMemoryKind::Decision,
            text: "Use api_key=sk-runtime-secret when replaying approved memory.".into(),
            review_state: db::project_store::AgentMemoryReviewState::Approved,
            enabled: true,
            confidence: Some(91),
            source_run_id: None,
            source_item_ids: Vec::new(),
            diagnostic: None,
            created_at: "2026-04-26T12:00:00Z".into(),
        },
    )
    .expect("seed approved memory");
    let seeded_memories = db::project_store::list_approved_agent_memories(
        &repo_root,
        &project_id,
        Some(db::project_store::DEFAULT_AGENT_SESSION_ID),
    )
    .expect("list approved memories");
    assert_eq!(seeded_memories.len(), 1);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-1".into(),
        prompt: "Please inspect the file.\ntool:read src/tracked.txt".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent task succeeds");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert!(snapshot.run.system_prompt.contains("xero-owned-agent-v1"));
    assert!(snapshot
        .run
        .system_prompt
        .contains("Instruction hierarchy:"));
    assert!(snapshot
        .run
        .system_prompt
        .contains("Final response contract:"));
    assert!(snapshot
        .run
        .system_prompt
        .contains("Repository instructions (project-owned, lower priority than Xero policy"));
    assert!(snapshot
        .run
        .system_prompt
        .contains("--- BEGIN PROJECT INSTRUCTIONS: AGENTS.md ---"));
    assert!(snapshot
        .run
        .system_prompt
        .contains("Keep test work repo-local."));
    assert!(snapshot.run.system_prompt.contains("Approved memory:"));
    assert!(
        snapshot
            .run
            .system_prompt
            .contains("retrieve it through `project_context`"),
        "approved memory policy should point agents at the durable-context tool"
    );
    assert!(!snapshot.run.system_prompt.contains("Use api_key="));
    assert!(!snapshot.run.system_prompt.contains("sk-runtime-secret"));
    assert!(snapshot.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::User
            && message.content.contains("tool:read src/tracked.txt")
    }));
    assert!(snapshot.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::Tool
            && message.content.contains("\"toolName\":\"read\"")
    }));

    let event_kinds = snapshot
        .events
        .iter()
        .map(|event| event.event_kind.clone())
        .collect::<Vec<_>>();
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::ValidationStarted));
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::ToolRegistrySnapshot));
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::ToolStarted));
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::ToolCompleted));
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::RunCompleted));
    let registry_event = snapshot
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::ToolRegistrySnapshot
        })
        .expect("tool registry event");
    let registry_payload: serde_json::Value =
        serde_json::from_str(&registry_event.payload_json).expect("registry payload");
    assert_eq!(registry_payload["kind"], "active_tool_registry");
    assert!(registry_payload["toolNames"]
        .as_array()
        .expect("tool names")
        .iter()
        .any(|name| name == "tool_search"));
    assert!(registry_payload["catalog"]
        .as_array()
        .expect("tool catalog metadata")
        .iter()
        .any(|entry| entry["toolName"] == "tool_search"
            && entry["activationGroups"]
                .as_array()
                .expect("activation groups")
                .iter()
                .any(|group| group == "core")
            && entry["riskClass"] == "observe"));

    assert_eq!(snapshot.tool_calls.len(), 1);
    let tool_call = &snapshot.tool_calls[0];
    assert_eq!(tool_call.tool_name, "read");
    assert_eq!(
        tool_call.state,
        db::project_store::AgentToolCallState::Succeeded
    );
    assert!(tool_call
        .result_json
        .as_deref()
        .is_some_and(|result| result.contains("alpha\\nbeta\\n")));

    let first_event_id = snapshot.events.first().expect("first event").id;
    let events_after_first = db::project_store::read_agent_events_after(
        &repo_root,
        &project_id,
        &snapshot.run.run_id,
        first_event_id,
        100,
    )
    .expect("read persisted events after first event");
    assert!(events_after_first
        .iter()
        .all(|event| event.id > first_event_id));
    assert!(events_after_first
        .iter()
        .any(|event| event.event_kind == db::project_store::AgentRunEventKind::RunCompleted));
}

#[test]
fn owned_agent_heartbeat_touch_updates_running_run_liveness() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let run_id = "owned-run-heartbeat-1";
    create_owned_agent_run(&OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: run_id.into(),
        prompt: "Prepare a heartbeat-only run.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("create owned agent run");

    let heartbeat = "2026-04-25T12:34:56Z";
    db::project_store::touch_agent_run_heartbeat(&repo_root, &project_id, run_id, heartbeat)
        .expect("touch owned agent heartbeat");
    let snapshot =
        db::project_store::load_agent_run(&repo_root, &project_id, run_id).expect("load run");
    assert_eq!(snapshot.run.last_heartbeat_at.as_deref(), Some(heartbeat));
    assert_eq!(snapshot.run.updated_at, heartbeat);
}

#[test]
fn owned_agent_continuation_blocks_context_handoff_without_mutating_messages() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "owned-run-context-budget-1";
    let provider_config = AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
        provider_id: "openai_api".into(),
        model_id: "gpt-4.1-mini".into(),
        base_url: "http://127.0.0.1:9/v1".into(),
        api_key: None,
        api_version: None,
        timeout_ms: 1,
    });
    let create_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for initial run");
    create_owned_agent_run(&OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: run_id.into(),
        prompt: "Prepare a budget-guarded run.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: create_tool_runtime,
        provider_config: provider_config.clone(),
    })
    .expect("create owned agent run");
    db::project_store::upsert_agent_context_policy_settings(
        &repo_root,
        &db::project_store::NewAgentContextPolicySettingsRecord {
            project_id: project_id.clone(),
            scope: db::project_store::AgentContextPolicySettingsScope::Project,
            agent_session_id: None,
            auto_compact_enabled: false,
            auto_handoff_enabled: false,
            compact_threshold_percent: 75,
            handoff_threshold_percent: 90,
            raw_tail_message_count: 8,
            updated_at: "2026-04-26T00:00:00Z".into(),
        },
    )
    .expect("disable automatic context handoff for prompt mutation guard");
    let before = db::project_store::load_agent_run(&repo_root, &project_id, run_id)
        .expect("load run before blocked continuation");
    let before_message_count = before.messages.len();

    let continue_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for continuation");
    let huge_prompt = "x".repeat(700_000);
    let error = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: run_id.into(),
        prompt: huge_prompt.clone(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: continue_tool_runtime,
        provider_config,
        answer_pending_actions: false,
        auto_compact: None,
    })
    .expect_err("blocked context handoff should reject before prompt mutation");

    assert_eq!(error.code, "agent_context_handoff_blocked");
    let after = db::project_store::load_agent_run(&repo_root, &project_id, run_id)
        .expect("load run after blocked continuation");
    assert_eq!(after.messages.len(), before_message_count);
    assert!(!after
        .messages
        .iter()
        .any(|message| message.content == huge_prompt));
    assert_eq!(after.run.status, before.run.status);
    let handoffs =
        db::project_store::list_agent_handoff_lineage_for_source(&repo_root, &project_id, run_id)
            .expect("list handoff lineage after blocked continuation");
    assert!(handoffs.is_empty());
}

#[test]
fn owned_agent_continuation_replays_compacted_history_with_raw_tail() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "owned-run-compacted-replay-1";
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");
    let initial = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: run_id.into(),
        prompt: "Inspect before compact.\ntool:read src/tracked.txt".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("initial owned-agent run");
    assert_eq!(
        initial.run.status,
        db::project_store::AgentRunStatus::Completed
    );

    let compacted = compact_session_history(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CompactSessionHistoryRequestDto {
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: Some(run_id.into()),
            raw_tail_message_count: Some(2),
        },
    )
    .expect("compact owned-agent run");
    assert!(compacted
        .context_snapshot
        .contributors
        .iter()
        .any(|contributor| contributor.kind
            == xero_desktop_lib::commands::SessionContextContributorKindDto::CompactionSummary));

    let continue_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for compacted continuation");
    let continued = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: run_id.into(),
        prompt: "Continue after compaction.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: continue_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        answer_pending_actions: false,
        auto_compact: None,
    })
    .expect("continue compacted owned-agent run");

    assert_eq!(
        continued.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert!(continued.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::User
            && message.content == "Continue after compaction."
    }));
}

#[test]
fn owned_agent_compacted_replay_rejects_changed_covered_source() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "owned-run-compacted-mismatch-1";
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");
    run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: run_id.into(),
        prompt: "Inspect before tamper.\ntool:read src/tracked.txt".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("initial owned-agent run");
    let compacted = compact_session_history(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CompactSessionHistoryRequestDto {
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: Some(run_id.into()),
            raw_tail_message_count: Some(2),
        },
    )
    .expect("compact owned-agent run");
    let covered_message_id = compacted
        .compaction
        .covered_message_start_id
        .expect("covered message id");
    let connection =
        Connection::open(db::database_path_for_repo(&repo_root)).expect("open runtime db");
    connection
        .execute(
            r#"
            UPDATE agent_messages
            SET content = content || ' tampered'
            WHERE project_id = ?1
              AND run_id = ?2
              AND id = ?3
            "#,
            params![project_id, run_id, covered_message_id],
        )
        .expect("tamper covered message");

    let continue_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for tampered continuation");
    let error = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: run_id.into(),
        prompt: "Continue after tamper.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: continue_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        answer_pending_actions: false,
        auto_compact: None,
    })
    .expect_err("covered transcript mutation should reject compacted replay");

    assert_eq!(error.code, "agent_compaction_source_mismatch");
    let snapshot = db::project_store::load_agent_run(&repo_root, &project_id, run_id)
        .expect("load tampered run");
    assert!(!snapshot
        .messages
        .iter()
        .any(|message| message.content == "Continue after tamper."));
}

#[test]
fn owned_agent_auto_compacts_before_continuation_when_threshold_is_reached() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "owned-run-auto-compact-1";
    let create_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for auto-compact source");
    create_owned_agent_run(&OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: run_id.into(),
        prompt: "Prepare an auto-compact source run.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: create_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("create auto-compact source run");
    append_auto_compact_fixture_messages(
        &repo_root,
        &project_id,
        run_id,
        "2026-04-26T17:00",
        8,
        4_000,
    );
    db::project_store::update_agent_run_status(
        &repo_root,
        &project_id,
        run_id,
        db::project_store::AgentRunStatus::Completed,
        None,
        "2026-04-26T17:00:50Z",
    )
    .expect("complete auto-compact source run");
    let before = db::project_store::load_agent_run(&repo_root, &project_id, run_id)
        .expect("load auto-compact source run");
    let before_message_count = before.messages.len();

    let continue_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for auto-compact continuation");
    let continued = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: run_id.into(),
        prompt: "Continue after automatic compaction.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: continue_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        answer_pending_actions: false,
        auto_compact: Some(AgentAutoCompactPreference {
            enabled: true,
            threshold_percent: Some(1),
            raw_tail_message_count: Some(2),
        }),
    })
    .expect("auto-compact continuation should succeed");

    assert_eq!(
        continued.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert!(continued.messages.len() > before_message_count);
    assert!(continued.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::User
            && message.content == "Continue after automatic compaction."
    }));
    let compactions = db::project_store::list_agent_compactions(
        &repo_root,
        &project_id,
        db::project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect("list auto compactions");
    assert_eq!(compactions.len(), 1);
    assert_eq!(
        compactions[0].trigger,
        db::project_store::AgentCompactionTrigger::Auto
    );
    assert_eq!(
        compactions[0].policy_reason,
        "auto_compact_threshold_reached"
    );
    assert_eq!(compactions[0].raw_tail_message_count, 2);
    assert!(continued.events.iter().any(|event| {
        event.event_kind == db::project_store::AgentRunEventKind::ValidationCompleted
            && serde_json::from_str::<serde_json::Value>(&event.payload_json)
                .is_ok_and(|payload| payload.get("label") == Some(&json!("auto_compact")))
    }));
}

#[test]
fn owned_agent_auto_compact_provider_failure_does_not_mutate_history() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "owned-run-auto-compact-failure-1";
    db::project_store::insert_agent_run(
        &repo_root,
        &db::project_store::NewAgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            agent_definition_version: None,
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: run_id.into(),
            provider_id: "openai_api".into(),
            model_id: "gpt-5.4".into(),
            prompt: "Prepare an auto-compact failure source.".into(),
            system_prompt: "You are Xero.".into(),
            now: "2026-04-26T18:00:00Z".into(),
        },
    )
    .expect("insert auto-compact failure source run");
    db::project_store::append_agent_message(
        &repo_root,
        &db::project_store::NewAgentMessageRecord {
            project_id: project_id.clone(),
            run_id: run_id.into(),
            role: db::project_store::AgentMessageRole::System,
            content: "You are Xero.".into(),
            created_at: "2026-04-26T18:00:01Z".into(),
            attachments: Vec::new(),
        },
    )
    .expect("append failure source system message");
    db::project_store::append_agent_message(
        &repo_root,
        &db::project_store::NewAgentMessageRecord {
            project_id: project_id.clone(),
            run_id: run_id.into(),
            role: db::project_store::AgentMessageRole::User,
            content: "Prepare an auto-compact failure source.".into(),
            created_at: "2026-04-26T18:00:02Z".into(),
            attachments: Vec::new(),
        },
    )
    .expect("append failure source user message");
    append_auto_compact_fixture_messages(
        &repo_root,
        &project_id,
        run_id,
        "2026-04-26T18:00",
        6,
        4_000,
    );
    db::project_store::update_agent_run_status(
        &repo_root,
        &project_id,
        run_id,
        db::project_store::AgentRunStatus::Completed,
        None,
        "2026-04-26T18:00:50Z",
    )
    .expect("complete auto-compact failure source run");
    let before = db::project_store::load_agent_run(&repo_root, &project_id, run_id)
        .expect("load failure source before continuation");
    let before_message_count = before.messages.len();

    let continue_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for failed auto-compact continuation");
    let error = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: run_id.into(),
        prompt: "This should not be appended.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: continue_tool_runtime,
        provider_config: AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
            provider_id: "openai_api".into(),
            model_id: "gpt-5.4".into(),
            base_url: "http://127.0.0.1:9/v1".into(),
            api_key: None,
            api_version: None,
            timeout_ms: 50,
        }),
        answer_pending_actions: false,
        auto_compact: Some(AgentAutoCompactPreference {
            enabled: true,
            threshold_percent: Some(1),
            raw_tail_message_count: Some(2),
        }),
    })
    .expect_err("provider compaction failure should reject before mutation");

    assert!(error.retryable || error.code.contains("provider"));
    let after = db::project_store::load_agent_run(&repo_root, &project_id, run_id)
        .expect("load failure source after continuation");
    assert_eq!(after.messages.len(), before_message_count);
    assert!(!after
        .messages
        .iter()
        .any(|message| message.content == "This should not be appended."));
    assert_eq!(after.run.status, before.run.status);
    assert!(db::project_store::list_agent_compactions(
        &repo_root,
        &project_id,
        db::project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect("list compactions after failed auto compact")
    .is_empty());
}

#[test]
fn owned_agent_plan_mode_allows_read_only_tool_call() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root,
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-plan-run-1".into(),
        prompt: "Please inspect the file.\ntool:read src/tracked.txt".into(),
        attachments: Vec::new(),
        controls: Some(RuntimeRunControlInputDto {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: None,
            provider_profile_id: None,
            model_id: "fake-model".into(),
            thinking_effort: None,
            approval_mode: RuntimeRunApprovalModeDto::Yolo,
            plan_mode_required: true,
        }),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("plan-mode read-only run should succeed");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert!(snapshot.tool_calls.iter().any(|tool_call| {
        tool_call.tool_name == "read"
            && tool_call.state == db::project_store::AgentToolCallState::Succeeded
    }));
    assert!(snapshot.action_requests.is_empty());
}

#[test]
fn owned_agent_write_tools_persist_file_change_hashes() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-write-1".into(),
        prompt: "Please update the tracked file.\ntool:read src/tracked.txt\ntool:write src/tracked.txt gamma\n".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent write task succeeds");

    assert_eq!(snapshot.file_changes.len(), 1);
    let file_change = &snapshot.file_changes[0];
    assert_eq!(file_change.path, "src/tracked.txt");
    assert_eq!(file_change.operation, "write");
    assert!(file_change
        .old_hash
        .as_deref()
        .is_some_and(|hash| hash.len() == 64));
    assert!(file_change
        .new_hash
        .as_deref()
        .is_some_and(|hash| hash.len() == 64));
    assert_ne!(file_change.old_hash, file_change.new_hash);

    let file_changed = snapshot
        .events
        .iter()
        .find(|event| event.event_kind == db::project_store::AgentRunEventKind::FileChanged)
        .expect("file_changed event");
    let payload: serde_json::Value =
        serde_json::from_str(&file_changed.payload_json).expect("file change payload JSON");
    assert_eq!(payload["path"], "src/tracked.txt");
    assert_eq!(payload["operation"], "write");
    assert!(payload["oldHash"].as_str().is_some());
    assert!(payload["newHash"].as_str().is_some());

    let updated = fs::read_to_string(repo_root.join("src").join("tracked.txt"))
        .expect("updated tracked file");
    assert_eq!(updated, "gamma");

    assert_eq!(snapshot.checkpoints.len(), 1);
    let checkpoint = &snapshot.checkpoints[0];
    assert_eq!(checkpoint.checkpoint_kind, "tool");
    let payload: serde_json::Value = serde_json::from_str(
        checkpoint
            .payload_json
            .as_deref()
            .expect("rollback checkpoint payload"),
    )
    .expect("rollback checkpoint payload JSON");
    assert_eq!(payload["kind"], "file_rollback");
    assert_eq!(payload["path"], "src/tracked.txt");
    assert_eq!(payload["operation"], "write");
    assert_eq!(payload["oldContentBase64"], "YWxwaGEKYmV0YQo=");
}

#[test]
fn owned_agent_omits_sensitive_file_content_from_rollback_checkpoints() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::write(repo_root.join(".env"), "OPENAI_API_KEY=sk-live-secret\n").expect("seed env file");
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root,
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-sensitive-rollback-1".into(),
        prompt: "Please update the env file.\ntool:read .env\ntool:write .env REDACTED=1\n".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent env write task succeeds");

    let checkpoint = snapshot
        .checkpoints
        .iter()
        .find(|checkpoint| checkpoint.summary.contains(".env"))
        .expect("env rollback checkpoint");
    let payload: serde_json::Value = serde_json::from_str(
        checkpoint
            .payload_json
            .as_deref()
            .expect("rollback checkpoint payload"),
    )
    .expect("rollback checkpoint payload JSON");
    assert_eq!(payload["path"], ".env");
    assert!(payload["oldHash"].as_str().is_some());
    assert!(payload["oldContentBase64"].is_null());
    assert_eq!(payload["oldContentOmittedReason"], "sensitive_path");
}

#[test]
fn owned_agent_refuses_unobserved_existing_file_writes() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root,
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-unobserved-write-1".into(),
        prompt: "Please update the tracked file.\ntool:write src/tracked.txt gamma\n".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent run should persist failed safety decision");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Paused
    );
    assert_eq!(snapshot.file_changes.len(), 0);
    assert!(snapshot.tool_calls.iter().any(|tool_call| {
        tool_call.tool_name == "write"
            && tool_call.state == db::project_store::AgentToolCallState::Failed
            && tool_call
                .error
                .as_ref()
                .is_some_and(|error| error.code == "agent_file_write_requires_observation")
    }));
    assert!(snapshot
        .events
        .iter()
        .any(|event| event.event_kind == db::project_store::AgentRunEventKind::ActionRequired));
    assert_eq!(snapshot.action_requests.len(), 1);
    assert_eq!(snapshot.action_requests[0].status, "pending");
    assert_eq!(snapshot.action_requests[0].action_type, "safety_boundary");
}

#[test]
fn owned_agent_resume_replays_answered_file_safety_tool_call() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let failed = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-approved-replay-1".into(),
        prompt: "Please update the tracked file.\ntool:write src/tracked.txt gamma\n".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent run should persist failed safety decision");
    assert_eq!(failed.run.status, db::project_store::AgentRunStatus::Paused);
    assert_eq!(
        fs::read_to_string(repo_root.join("src").join("tracked.txt"))
            .expect("tracked file before approval"),
        "alpha\nbeta\n"
    );

    let approved_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for approved replay");
    let resumed = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: "owned-run-approved-replay-1".into(),
        prompt: "Approved. Continue.".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime: approved_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        answer_pending_actions: true,
        auto_compact: None,
    })
    .expect("approved safety action should replay original tool call");

    assert_eq!(
        resumed.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("src").join("tracked.txt"))
            .expect("tracked file after approved replay"),
        "gamma"
    );
    let write_call = resumed
        .tool_calls
        .iter()
        .find(|tool_call| tool_call.tool_name == "write")
        .expect("write tool call should remain in journal");
    assert_eq!(
        write_call.state,
        db::project_store::AgentToolCallState::Succeeded
    );
    assert!(resumed.action_requests.iter().all(|action| {
        action.status == "answered" && action.response.as_deref() == Some("Approved. Continue.")
    }));
    assert!(resumed.events.iter().any(|event| {
        event.event_kind == db::project_store::AgentRunEventKind::ToolStarted
            && event.payload_json.contains("\"approvedReplay\":true")
    }));
    assert!(resumed.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::Tool
            && message.content.contains("\"toolName\":\"write\"")
    }));
}

#[test]
fn owned_agent_refuses_stale_file_writes_after_observation_changes() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-stale-write-1".into(),
        prompt: "Please update safely.\ntool:read src/tracked.txt\ntool:command_sh printf outside > src/tracked.txt\ntool:write src/tracked.txt gamma\n".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent run should persist stale-write safety decision");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Paused
    );
    assert!(snapshot.tool_calls.iter().any(|tool_call| {
        tool_call.tool_name == "command"
            && tool_call.state == db::project_store::AgentToolCallState::Succeeded
    }));
    assert!(snapshot.tool_calls.iter().any(|tool_call| {
        tool_call.tool_name == "write"
            && tool_call.state == db::project_store::AgentToolCallState::Failed
            && tool_call
                .error
                .as_ref()
                .is_some_and(|error| error.code == "agent_file_changed_since_observed")
    }));

    let updated = fs::read_to_string(repo_root.join("src").join("tracked.txt"))
        .expect("tracked file after stale write refusal");
    assert_eq!(updated, "outside");
}

#[test]
fn owned_agent_resume_marks_interrupted_tool_calls_before_continuation() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "owned-run-interrupted-tool-resume-1";

    let create_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for initial run");
    create_owned_agent_run(&OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: run_id.into(),
        prompt: "Initial interrupted run.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: create_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("create owned agent run");

    db::project_store::start_agent_tool_call(
        &repo_root,
        &db::project_store::AgentToolCallStartRecord {
            project_id: project_id.clone(),
            run_id: run_id.into(),
            tool_call_id: "tool-call-interrupted-read".into(),
            tool_name: "read".into(),
            input_json: json!({ "path": "src/tracked.txt" }).to_string(),
            started_at: "2026-04-24T00:00:00Z".into(),
        },
    )
    .expect("seed persisted running tool call");

    let continue_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for continuation");
    let snapshot = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: run_id.into(),
        prompt: "Continue after restart.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: continue_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        answer_pending_actions: false,
        auto_compact: None,
    })
    .expect("resume interrupted owned agent run");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    let interrupted = snapshot
        .tool_calls
        .iter()
        .find(|tool_call| tool_call.tool_call_id == "tool-call-interrupted-read")
        .expect("interrupted tool call should remain in journal");
    assert_eq!(
        interrupted.state,
        db::project_store::AgentToolCallState::Failed
    );
    assert!(interrupted.error.as_ref().is_some_and(|error| {
        error.code == "agent_tool_call_interrupted"
            && error.message.contains("interrupted before resuming")
    }));
    assert!(snapshot.events.iter().any(|event| {
        event.event_kind == db::project_store::AgentRunEventKind::ToolCompleted
            && event.payload_json.contains("agent_tool_call_interrupted")
    }));
    assert!(snapshot.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::User
            && message.content == "Continue after restart."
    }));
}

#[test]
fn owned_agent_command_tools_emit_command_output_events() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root,
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-command-1".into(),
        prompt: "Please prove command output streaming.\ntool:command_echo hello-xero".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent command task succeeds");

    let command_output = snapshot
        .events
        .iter()
        .find(|event| event.event_kind == db::project_store::AgentRunEventKind::CommandOutput)
        .expect("command_output event");
    let payload: serde_json::Value =
        serde_json::from_str(&command_output.payload_json).expect("command output payload JSON");
    assert_eq!(payload["argv"], json!(["echo", "hello-xero"]));
    assert_eq!(payload["stdout"], "hello-xero");
    assert_eq!(payload["spawned"], true);
    assert_eq!(payload["exitCode"], 0);
}

#[test]
fn owned_agent_resume_replays_answered_command_approval_tool_call() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime");

    let controls = suggest_controls_input();
    let initial = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-command-approval-replay-1".into(),
        prompt: "Please run an approved command.\ntool:command_sh printf approved > approved-command.txt".into(),
        attachments: Vec::new(),
        controls: Some(controls.clone()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
    })
    .expect("owned agent command approval run should complete with pending action");

    assert_eq!(
        initial.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert!(!repo_root.join("approved-command.txt").exists());
    assert_eq!(initial.action_requests.len(), 1);
    assert_eq!(initial.action_requests[0].status, "pending");
    assert_eq!(initial.action_requests[0].action_type, "command_approval");

    let approved_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for approved command replay");
    let resumed = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: "owned-run-command-approval-replay-1".into(),
        prompt: "Approved. Run it now.".into(),
        attachments: Vec::new(),
        controls: Some(controls),
        tool_runtime: approved_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        answer_pending_actions: true,
        auto_compact: None,
    })
    .expect("approved command action should replay original tool call");

    assert_eq!(
        resumed.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("approved-command.txt"))
            .expect("approved command output file"),
        "approved"
    );
    let spawned_values = resumed
        .events
        .iter()
        .filter(|event| event.event_kind == db::project_store::AgentRunEventKind::CommandOutput)
        .map(|event| {
            serde_json::from_str::<serde_json::Value>(&event.payload_json)
                .expect("command output payload JSON")["spawned"]
                .as_bool()
                .expect("spawned bool")
        })
        .collect::<Vec<_>>();
    assert!(spawned_values.contains(&false));
    assert!(spawned_values.contains(&true));
    assert!(resumed.events.iter().any(|event| {
        event.event_kind == db::project_store::AgentRunEventKind::ToolStarted
            && event.payload_json.contains("\"approvedReplay\":true")
    }));
    let command_call = resumed
        .tool_calls
        .iter()
        .find(|tool_call| tool_call.tool_name == "command")
        .expect("command tool call should remain in journal");
    assert!(command_call
        .result_json
        .as_deref()
        .is_some_and(|result| result.contains("\"spawned\":true")));
}

#[test]
fn owned_agent_command_sessions_start_capture_and_stop_long_running_processes() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime")
    .with_runtime_run_controls(yolo_controls());

    let started = tool_runtime
        .command_session_start(AutonomousCommandSessionStartRequest {
            argv: vec!["sh".into(), "-c".into(), "printf ready; sleep 5".into()],
            cwd: None,
            timeout_ms: Some(1_000),
        })
        .expect("start command session");
    let AutonomousToolOutput::CommandSession(start_output) = started.output else {
        panic!("expected command session output");
    };
    assert_eq!(
        start_output.operation,
        AutonomousCommandSessionOperation::Start
    );
    assert!(start_output.spawned);
    assert!(start_output.running);
    assert!(start_output
        .chunks
        .iter()
        .any(|chunk| chunk.text.as_deref() == Some("ready")));

    let stopped = tool_runtime
        .command_session_stop(AutonomousCommandSessionStopRequest {
            session_id: start_output.session_id.clone(),
        })
        .expect("stop command session");
    let AutonomousToolOutput::CommandSession(stop_output) = stopped.output else {
        panic!("expected command session output");
    };
    assert_eq!(
        stop_output.operation,
        AutonomousCommandSessionOperation::Stop
    );
    assert!(!stop_output.running);
    assert_eq!(stop_output.session_id, start_output.session_id);
}

#[test]
fn owned_agent_command_session_stop_terminates_child_process_tree() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);
    let marker_path = root.path().join("escaped-child-process");
    let script = format!(
        "(sleep 1; printf escaped > '{}') & wait",
        marker_path.display()
    );
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime")
    .with_runtime_run_controls(yolo_controls());

    let started = tool_runtime
        .command_session_start(AutonomousCommandSessionStartRequest {
            argv: vec!["sh".into(), "-c".into(), script],
            cwd: None,
            timeout_ms: Some(1_000),
        })
        .expect("start command session");
    let AutonomousToolOutput::CommandSession(start_output) = started.output else {
        panic!("expected command session output");
    };
    assert!(start_output.running);

    tool_runtime
        .command_session_stop(AutonomousCommandSessionStopRequest {
            session_id: start_output.session_id,
        })
        .expect("stop command session");

    thread::sleep(Duration::from_millis(1_300));
    assert!(
        !marker_path.exists(),
        "grandchild process survived command-session stop and wrote {}",
        marker_path.display()
    );
}

#[test]
fn owned_agent_one_shot_command_cleans_up_background_children_after_root_exit() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let marker_path = repo_root.join("escaped-child-process");
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime")
    .with_runtime_run_controls(yolo_controls());

    let result = tool_runtime
        .command(AutonomousCommandRequest {
            argv: vec![
                "sh".into(),
                "-c".into(),
                "(sleep 1; printf escaped > escaped-child-process) &".into(),
            ],
            cwd: None,
            timeout_ms: Some(2_000),
        })
        .expect("one-shot shell command should run");
    let AutonomousToolOutput::Command(output) = result.output else {
        panic!("expected command output");
    };
    assert!(output.spawned);
    assert_eq!(output.exit_code, Some(0));

    thread::sleep(Duration::from_millis(1_300));
    assert!(
        !marker_path.exists(),
        "grandchild process survived one-shot command cleanup and wrote {}",
        marker_path.display()
    );
}

#[test]
fn owned_agent_shell_commands_with_sensitive_expansion_require_approval() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime")
    .with_runtime_run_controls(yolo_controls());

    let result = tool_runtime
        .command(AutonomousCommandRequest {
            argv: vec![
                "sh".into(),
                "-c".into(),
                "printf ${XERO_AGENT_SECRET_TEST_TOKEN:-missing}".into(),
            ],
            cwd: None,
            timeout_ms: Some(1_000),
        })
        .expect("sensitive shell command should produce approval request");
    let AutonomousToolOutput::Command(output) = result.output else {
        panic!("expected command output");
    };
    assert!(!output.spawned);
    assert_eq!(output.policy.code, "policy_escalated_sensitive_shell");
}

#[test]
fn owned_agent_commands_run_with_sanitized_environment_even_after_approval() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime")
    .with_runtime_run_controls(yolo_controls());

    std::env::set_var("XERO_AGENT_SECRET_TEST_TOKEN", "super-secret");
    let result = tool_runtime.command_with_operator_approval(AutonomousCommandRequest {
        argv: vec![
            "sh".into(),
            "-c".into(),
            "printf ${XERO_AGENT_SECRET_TEST_TOKEN:-missing}".into(),
        ],
        cwd: None,
        timeout_ms: Some(1_000),
    });
    std::env::remove_var("XERO_AGENT_SECRET_TEST_TOKEN");

    let result = result.expect("approved command should run with scrubbed env");
    let AutonomousToolOutput::Command(output) = result.output else {
        panic!("expected command output");
    };
    assert!(output.spawned);
    assert_eq!(output.stdout.as_deref(), Some("missing"));
    assert!(!output.stdout_redacted);
}

#[test]
fn owned_agent_command_output_redacts_common_secret_markers() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime")
    .with_runtime_run_controls(yolo_controls());

    let result = tool_runtime
        .command(AutonomousCommandRequest {
            argv: vec!["echo".into(), "github_pat_redacted_example".into()],
            cwd: None,
            timeout_ms: Some(1_000),
        })
        .expect("echo command should run");
    let AutonomousToolOutput::Command(output) = result.output else {
        panic!("expected command output");
    };
    assert!(output.stdout_redacted);
    assert_eq!(
        output.stdout.as_deref(),
        Some("Command output was redacted before durable persistence.")
    );
    assert_eq!(output.argv, vec!["echo", "[REDACTED]"]);

    let npm_token = tool_runtime
        .command(AutonomousCommandRequest {
            argv: vec!["echo".into(), "_authToken=npm_secret_value".into()],
            cwd: None,
            timeout_ms: Some(1_000),
        })
        .expect("echo command should run");
    let AutonomousToolOutput::Command(output) = npm_token.output else {
        panic!("expected command output");
    };
    assert!(output.stdout_redacted);
    assert_eq!(
        output.stdout.as_deref(),
        Some("Command output was redacted before durable persistence.")
    );
    assert_eq!(output.argv, vec!["echo", "[REDACTED]"]);
}

#[test]
fn owned_agent_command_sessions_enforce_concurrency_limit() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime")
    .with_runtime_run_controls(yolo_controls());

    let mut session_ids = Vec::new();
    for _ in 0..8 {
        let started = tool_runtime
            .command_session_start(AutonomousCommandSessionStartRequest {
                argv: vec!["sh".into(), "-c".into(), "sleep 5".into()],
                cwd: None,
                timeout_ms: Some(1_000),
            })
            .expect("start command session");
        let AutonomousToolOutput::CommandSession(output) = started.output else {
            panic!("expected command session output");
        };
        session_ids.push(output.session_id);
    }

    let error = tool_runtime
        .command_session_start(AutonomousCommandSessionStartRequest {
            argv: vec!["sh".into(), "-c".into(), "sleep 5".into()],
            cwd: None,
            timeout_ms: Some(1_000),
        })
        .expect_err("ninth command session should be refused");
    assert_eq!(error.code, "autonomous_tool_command_session_limit_reached");

    for session_id in session_ids {
        let _ =
            tool_runtime.command_session_stop(AutonomousCommandSessionStopRequest { session_id });
    }
}

#[test]
fn start_runtime_run_defaults_to_owned_agent_runtime() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime_run = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id,
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            initial_controls: None,
            initial_prompt: None,
            initial_attachments: Vec::new(),
        },
    )
    .expect("start runtime run should create owned agent runtime");

    assert_eq!(runtime_run.runtime_kind, "owned_agent");
    assert_eq!(runtime_run.supervisor_kind, "owned_agent");
    assert_eq!(runtime_run.transport.kind, "internal");
    assert_eq!(runtime_run.transport.endpoint, "xero://owned-agent");
    assert_eq!(
        runtime_run.controls.active.runtime_agent_id,
        RuntimeAgentIdDto::Ask
    );
    assert_eq!(
        runtime_run.controls.active.approval_mode,
        RuntimeRunApprovalModeDto::Suggest
    );
}

#[test]
fn start_runtime_run_initial_prompt_runs_owned_agent_task() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    let runtime_run = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            initial_controls: None,
            initial_prompt: Some("Inspect the tracked file.\ntool:read src/tracked.txt".into()),
            initial_attachments: Vec::new(),
        },
    )
    .expect("start runtime run should execute owned agent task from initial prompt");

    let agent_run = wait_for_agent_run_status(
        &repo_root,
        &project_id,
        &runtime_run.run_id,
        db::project_store::AgentRunStatus::Completed,
    );
    assert_eq!(
        agent_run.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert!(agent_run.tool_calls.iter().any(|tool_call| {
        tool_call.tool_name == "read"
            && tool_call.state == db::project_store::AgentToolCallState::Succeeded
    }));
}

#[test]
fn archive_agent_session_stops_idle_runtime_run_after_interaction() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let agent_session_id = db::project_store::DEFAULT_AGENT_SESSION_ID.to_string();

    let runtime_run = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            agent_session_id: agent_session_id.clone(),
            initial_controls: None,
            initial_prompt: Some("Inspect the tracked file.\ntool:read src/tracked.txt".into()),
            initial_attachments: Vec::new(),
        },
    )
    .expect("start runtime run should execute owned agent task from initial prompt");

    wait_for_agent_run_status(
        &repo_root,
        &project_id,
        &runtime_run.run_id,
        db::project_store::AgentRunStatus::Completed,
    );
    wait_for_agent_run_inactive(app.state::<DesktopState>().inner(), &runtime_run.run_id);

    let archived = archive_agent_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ArchiveAgentSessionRequestDto {
            project_id: project_id.clone(),
            agent_session_id: agent_session_id.clone(),
        },
    )
    .expect("archive should stop the idle owned runtime run first");

    assert!(archived.archived_at.is_some());
    let stopped = wait_for_runtime_run_status(
        &repo_root,
        &project_id,
        &agent_session_id,
        db::project_store::RuntimeRunStatus::Stopped,
    );
    assert_eq!(stopped.run.run_id, runtime_run.run_id);
    assert!(stopped.run.stopped_at.is_some());
}

#[test]
fn update_runtime_run_controls_prompt_drives_owned_agent_continuation() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    let runtime_run = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            initial_controls: None,
            initial_prompt: None,
            initial_attachments: Vec::new(),
        },
    )
    .expect("start runtime run should create owned agent runtime");

    let updated = update_runtime_run_controls(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpdateRuntimeRunControlsRequestDto {
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: runtime_run.run_id.clone(),
            controls: None,
            prompt: Some("Inspect the tracked file.\ntool:read src/tracked.txt".into()),
            attachments: Vec::new(),
            auto_compact: None,
        },
    )
    .expect("runtime prompt should start owned agent run");
    assert_eq!(updated.run_id, runtime_run.run_id);

    let agent_run = wait_for_agent_run_status(
        &repo_root,
        &project_id,
        &runtime_run.run_id,
        db::project_store::AgentRunStatus::Completed,
    );
    assert!(agent_run.tool_calls.iter().any(|tool_call| {
        tool_call.tool_name == "read"
            && tool_call.state == db::project_store::AgentToolCallState::Succeeded
    }));
    wait_for_agent_run_inactive(app.state::<DesktopState>().inner(), &runtime_run.run_id);

    update_runtime_run_controls(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpdateRuntimeRunControlsRequestDto {
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            run_id: runtime_run.run_id.clone(),
            controls: None,
            prompt: Some("Thanks, summarize the result.".into()),
            attachments: Vec::new(),
            auto_compact: None,
        },
    )
    .expect("runtime prompt should continue owned agent run");

    let continued = wait_for_agent_run_status(
        &repo_root,
        &project_id,
        &runtime_run.run_id,
        db::project_store::AgentRunStatus::Completed,
    );
    assert!(continued.messages.iter().any(|message| {
        message.role == db::project_store::AgentMessageRole::User
            && message.content == "Thanks, summarize the result."
    }));
}

#[test]
fn start_agent_task_returns_running_before_background_driver_finishes() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    let agent_run = start_agent_task(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartAgentTaskRequestDto {
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            prompt: "Run a slow command.\ntool:command_sh sleep 2".into(),
            controls: Some(yolo_controls_input()),
        },
    )
    .expect("start agent task should return initial running snapshot");

    assert_eq!(
        agent_run.status,
        xero_desktop_lib::commands::AgentRunStatusDto::Running
    );

    let completed = wait_for_agent_run_status(
        &repo_root,
        &project_id,
        &agent_run.run_id,
        db::project_store::AgentRunStatus::Completed,
    );
    assert!(completed.tool_calls.iter().any(|tool_call| {
        tool_call.tool_name == "command"
            && tool_call.state == db::project_store::AgentToolCallState::Succeeded
    }));
}

#[test]
fn cancel_agent_run_flips_background_task_cancellation_token() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    let agent_run = start_agent_task(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartAgentTaskRequestDto {
            project_id: project_id.clone(),
            agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
            prompt: "Run a cancellable command.\ntool:command_sh sleep 5".into(),
            controls: None,
        },
    )
    .expect("start cancellable agent task");

    thread::sleep(Duration::from_millis(150));
    let cancelled = cancel_agent_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        CancelAgentRunRequestDto {
            run_id: agent_run.run_id.clone(),
        },
    )
    .expect("cancel running agent task");
    assert_eq!(
        cancelled.status,
        xero_desktop_lib::commands::AgentRunStatusDto::Cancelled
    );

    let snapshot = wait_for_agent_run_status(
        &repo_root,
        &project_id,
        &agent_run.run_id,
        db::project_store::AgentRunStatus::Cancelled,
    );
    assert!(snapshot.events.iter().any(|event| {
        event.event_kind == db::project_store::AgentRunEventKind::RunFailed
            && event.payload_json.contains("agent_run_cancelled")
    }));
}
