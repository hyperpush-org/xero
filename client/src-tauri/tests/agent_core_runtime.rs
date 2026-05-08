use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use git2::{IndexAddOption, Repository, Signature};
use rusqlite::{params, Connection};
use serde_json::json;
use sha2::{Digest, Sha256};
use tauri::Manager;
use tempfile::TempDir;
use time::{format_description::well_known::Rfc3339, OffsetDateTime};
use xero_agent_core::{
    provider_capability_catalog, provider_preflight_snapshot, AgentRuntimeFacade,
    ApprovalDecisionRequest, CompactSessionRequest, ForkSessionRequest,
    ProductionReadinessFocusedTestResult, ProductionReadinessFocusedTestStatus,
    ProductionReadinessStatus, ProviderCapabilityCatalogInput, ProviderPreflightInput,
    ProviderPreflightRequiredFeatures, ProviderPreflightSource, RuntimeEventKind,
    RuntimeExecutionMode, RuntimeStoreDescriptor, RuntimeStoreKind, StaticToolHandler,
    ToolApprovalRequirement, ToolBudget, ToolCallInput, ToolDispatchConfig, ToolErrorCategory,
    ToolGroupExecutionMode, ToolHandlerOutput, ToolMutability, ToolRegistryV2,
    ToolSandboxRequirement, DEFAULT_PROVIDER_CATALOG_TTL_SECONDS,
    PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS,
};
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
        continue_owned_agent_run, create_owned_agent_run, drive_owned_agent_run,
        export_harness_contract, run_owned_agent_task, AgentAutoCompactPreference,
        AgentProviderConfig, AgentRunCancellationToken, AgentRunSupervisor, AgentToolCall,
        AutonomousCommandRequest, AutonomousCommandSessionOperation,
        AutonomousCommandSessionStartRequest, AutonomousCommandSessionStopRequest,
        AutonomousToolOutput, AutonomousToolRuntime, ContinueOwnedAgentRunRequest,
        DesktopAgentCoreRuntime, DesktopCompactSessionRequest, DesktopForkSessionRequest,
        DesktopRejectActionRequest, DesktopRunDriveMode, DesktopStartRunRequest,
        OpenAiCompatibleProviderConfig, OwnedAgentRunRequest, ToolRegistry, ToolRegistryOptions,
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

fn seed_workspace_index_ready(repo_root: &Path, project_id: &str) {
    let indexed_file = repo_root.join("src").join("indexed.rs");
    fs::write(
        &indexed_file,
        "pub fn semantic_workspace_index_ready() -> bool { true }\n",
    )
    .expect("write indexed source");

    let database_path = db::database_path_for_repo(repo_root);
    let connection = Connection::open(&database_path).expect("open project database");
    let now = "2026-05-05T12:00:00Z";
    let storage_path = database_path
        .parent()
        .expect("project database parent")
        .to_string_lossy()
        .into_owned();
    let head_sha = current_head_sha(repo_root);
    let indexed_paths = ["AGENTS.md", "src/indexed.rs"];

    connection
        .execute(
            r#"
            INSERT INTO workspace_index_metadata (
                project_id, status, index_version, root_path, storage_path, head_sha,
                worktree_fingerprint, total_files, indexed_files, skipped_files, stale_files,
                symbol_count, indexed_bytes, coverage_percent, diagnostics_json,
                started_at, completed_at, updated_at
            )
            VALUES (?1, 'ready', 1, ?2, ?3, ?4, 'test-fingerprint', ?5, ?5, 0, 0, 1, ?6, 100.0, '[]', ?7, ?7, ?7)
            ON CONFLICT(project_id) DO UPDATE SET
                status = excluded.status,
                root_path = excluded.root_path,
                storage_path = excluded.storage_path,
                head_sha = excluded.head_sha,
                worktree_fingerprint = excluded.worktree_fingerprint,
                total_files = excluded.total_files,
                indexed_files = excluded.indexed_files,
                skipped_files = excluded.skipped_files,
                stale_files = excluded.stale_files,
                symbol_count = excluded.symbol_count,
                indexed_bytes = excluded.indexed_bytes,
                coverage_percent = excluded.coverage_percent,
                diagnostics_json = excluded.diagnostics_json,
                started_at = excluded.started_at,
                completed_at = excluded.completed_at,
                updated_at = excluded.updated_at
            "#,
            params![
                project_id,
                repo_root.to_string_lossy().as_ref(),
                storage_path,
                head_sha,
                indexed_paths.len() as i64,
                indexed_paths
                    .iter()
                    .map(|path| fs::metadata(repo_root.join(path)).expect("metadata").len())
                    .sum::<u64>() as i64,
                now,
            ],
        )
        .expect("seed workspace index metadata");

    for path in indexed_paths {
        let absolute_path = repo_root.join(path);
        let content = fs::read_to_string(&absolute_path).expect("read indexed file");
        let metadata = fs::metadata(&absolute_path).expect("indexed file metadata");
        let content_hash = format!("{:x}", Sha256::digest(content.as_bytes()));
        let modified_at = metadata_modified_at(&metadata);
        let virtual_path = format!("/{}", path.replace('\\', "/"));
        let language = if path.ends_with(".md") {
            "markdown"
        } else {
            "rust"
        };
        connection
            .execute(
                r#"
                INSERT INTO workspace_index_files (
                    project_id, path, language, content_hash, modified_at, byte_length,
                    summary, snippet, symbols_json, imports_json, tests_json, routes_json,
                    commands_json, diffs_json, failures_json, embedding_json, embedding_model,
                    embedding_version, indexed_at
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, '[]', '[]', '[]', '[]', '[]', '[]', '[]', '[0.0]', 'test-embedding', 'v1', ?9)
                ON CONFLICT(project_id, path) DO UPDATE SET
                    language = excluded.language,
                    content_hash = excluded.content_hash,
                    modified_at = excluded.modified_at,
                    byte_length = excluded.byte_length,
                    summary = excluded.summary,
                    snippet = excluded.snippet,
                    embedding_json = excluded.embedding_json,
                    indexed_at = excluded.indexed_at
                "#,
                params![
                    project_id,
                    virtual_path,
                    language,
                    content_hash,
                    modified_at,
                    metadata.len() as i64,
                    format!("Indexed {path}."),
                    content,
                    now,
                ],
            )
            .expect("seed workspace index file row");
    }
}

fn metadata_modified_at(metadata: &fs::Metadata) -> String {
    metadata
        .modified()
        .ok()
        .map(OffsetDateTime::from)
        .and_then(|value| value.format(&Rfc3339).ok())
        .unwrap_or_else(|| "1970-01-01T00:00:00Z".into())
}

fn lifecycle_health_checks(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> Vec<serde_json::Value> {
    let snapshot =
        db::project_store::load_agent_environment_lifecycle_snapshot(repo_root, project_id, run_id)
            .expect("load lifecycle snapshot")
            .expect("lifecycle snapshot");
    serde_json::from_str::<Vec<serde_json::Value>>(&snapshot.health_checks_json)
        .expect("health checks json")
}

fn semantic_lifecycle_health_check(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> serde_json::Value {
    lifecycle_health_checks(repo_root, project_id, run_id)
        .into_iter()
        .find(|check| check["kind"] == "semantic_index_status")
        .expect("semantic lifecycle health check")
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

fn live_provider_preflight(
    provider_id: &str,
    model_id: &str,
) -> xero_agent_core::ProviderPreflightSnapshot {
    provider_preflight_snapshot(ProviderPreflightInput {
        profile_id: provider_id.into(),
        provider_id: provider_id.into(),
        model_id: model_id.into(),
        source: ProviderPreflightSource::LiveProbe,
        checked_at: "unix:1770000000".into(),
        age_seconds: Some(0),
        ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
        required_features: ProviderPreflightRequiredFeatures::owned_agent_text_turn(),
        capabilities: provider_capability_catalog(ProviderCapabilityCatalogInput {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            catalog_source: "live".into(),
            fetched_at: Some("unix:1770000000".into()),
            last_success_at: Some("unix:1770000000".into()),
            cache_age_seconds: Some(0),
            cache_ttl_seconds: Some(DEFAULT_PROVIDER_CATALOG_TTL_SECONDS),
            credential_proof: Some("none_required".into()),
            context_window_tokens: Some(128_000),
            max_output_tokens: Some(16_384),
            context_limit_source: Some("built_in_registry".into()),
            context_limit_confidence: Some("high".into()),
            thinking_supported: false,
            thinking_efforts: Vec::new(),
            thinking_default_effort: None,
        }),
        credential_ready: Some(true),
        endpoint_reachable: Some(true),
        model_available: Some(true),
        streaming_route_available: Some(true),
        tool_schema_accepted: Some(true),
        reasoning_controls_accepted: None,
        attachments_accepted: None,
        context_limit_known: Some(true),
        provider_error: None,
    })
}

fn production_readiness_focused_tests(
    status: ProductionReadinessFocusedTestStatus,
) -> Vec<ProductionReadinessFocusedTestResult> {
    PRODUCTION_READINESS_REQUIRED_TEST_COMMANDS
        .iter()
        .map(|command| ProductionReadinessFocusedTestResult {
            command: (*command).into(),
            status,
            summary: "focused test evidence fixture".into(),
            checked_at: Some("2026-05-05T12:00:00Z".into()),
        })
        .collect()
}

struct MockOpenAiCompatibleSseServer {
    base_url: String,
    handle: thread::JoinHandle<()>,
}

impl MockOpenAiCompatibleSseServer {
    fn start(responses: Vec<String>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock provider");
        let address = listener.local_addr().expect("mock provider address");
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept provider request");
                read_http_request(&mut stream);
                let reply = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    response.len(),
                    response
                );
                stream
                    .write_all(reply.as_bytes())
                    .expect("write provider response");
            }
        });
        Self {
            base_url: format!("http://{address}/v1"),
            handle,
        }
    }

    fn join(self) {
        self.handle.join().expect("mock provider thread");
    }
}

fn read_http_request(stream: &mut std::net::TcpStream) {
    let mut buffer = [0_u8; 4096];
    let mut request = Vec::new();
    loop {
        let read = stream.read(&mut buffer).expect("read provider request");
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
}

fn openai_chat_sse(chunks: Vec<serde_json::Value>) -> String {
    let mut lines = chunks
        .into_iter()
        .map(|chunk| format!("data: {chunk}\n\n"))
        .collect::<Vec<_>>();
    lines.push("data: [DONE]\n\n".into());
    lines.join("")
}

#[test]
fn tool_group_timeout_interrupts_hung_read_only_handler() {
    let mut registry = ToolRegistryV2::new();
    for name in ["slow_a", "slow_b"] {
        registry
            .register(StaticToolHandler::new(
                xero_agent_core::ToolDescriptorV2 {
                    name: name.into(),
                    description: "Slow read-only fixture.".into(),
                    input_schema: json!({
                        "type": "object",
                        "properties": {
                            "path": { "type": "string" }
                        },
                        "required": ["path"],
                        "additionalProperties": false
                    }),
                    capability_tags: vec!["fixture".into()],
                    effect_class: xero_agent_core::ToolEffectClass::FileRead,
                    mutability: ToolMutability::ReadOnly,
                    sandbox_requirement: xero_agent_core::ToolSandboxRequirement::None,
                    approval_requirement: xero_agent_core::ToolApprovalRequirement::Never,
                    telemetry_attributes: Default::default(),
                    result_truncation: xero_agent_core::ToolResultTruncationContract::default(),
                },
                |_context, _call| {
                    thread::sleep(Duration::from_millis(150));
                    Ok(ToolHandlerOutput::new("late", json!({ "ok": true })))
                },
            ))
            .expect("register slow read-only fixture");
    }
    let config = ToolDispatchConfig {
        budget: ToolBudget {
            max_wall_clock_time_per_tool_group_ms: 20,
            ..ToolBudget::default()
        },
        ..ToolDispatchConfig::default()
    };

    let started = Instant::now();
    let report = registry.dispatch_batch(
        &[
            ToolCallInput {
                tool_call_id: "call-1".into(),
                tool_name: "slow_a".into(),
                input: json!({ "path": "a" }),
            },
            ToolCallInput {
                tool_call_id: "call-2".into(),
                tool_name: "slow_b".into(),
                input: json!({ "path": "b" }),
            },
        ],
        &config,
    );

    assert!(
        started.elapsed() < Duration::from_millis(80),
        "read-only group timeout should return before sleeping handlers complete"
    );
    assert_eq!(
        report.groups[0].mode,
        ToolGroupExecutionMode::ParallelReadOnly
    );
    assert!(report.groups[0].timeout_error.is_some());
    assert!(report.groups[0].outcomes.iter().all(|outcome| {
        outcome
            .failure()
            .is_some_and(|failure| failure.error.category == ToolErrorCategory::Timeout)
    }));
}

fn wait_for_agent_run_status(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    status: db::project_store::AgentRunStatus,
) -> db::project_store::AgentRunSnapshotRecord {
    let deadline = Instant::now() + Duration::from_secs(30);
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
fn desktop_facade_reject_action_persists_decision_and_trace() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let state = app.state::<DesktopState>();
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "desktop-facade-reject-1";
    let tool_runtime =
        AutonomousToolRuntime::for_project(&app.handle().clone(), state.inner(), &project_id)
            .expect("build autonomous tool runtime");
    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());

    <DesktopAgentCoreRuntime as AgentRuntimeFacade>::start_run(
        &runtime,
        DesktopStartRunRequest {
            request: OwnedAgentRunRequest {
                repo_root: repo_root.clone(),
                project_id: project_id.clone(),
                agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
                run_id: run_id.into(),
                prompt: "Prepare a paused run for rejection.".into(),
                attachments: Vec::new(),
                controls: Some(yolo_controls_input()),
                tool_runtime,
                provider_config: AgentProviderConfig::Fake,
                provider_preflight: None,
            },
            drive_mode: DesktopRunDriveMode::CreateOnly,
        },
    )
    .expect("create desktop facade run");
    db::project_store::append_agent_action_request(
        &repo_root,
        &db::project_store::NewAgentActionRequestRecord {
            project_id: project_id.clone(),
            run_id: run_id.into(),
            action_id: "tool-call-denied".into(),
            action_type: "safety_boundary".into(),
            title: "Action required".into(),
            detail: "A mutating tool call needs operator input.".into(),
            created_at: "2026-05-04T10:00:00Z".into(),
        },
    )
    .expect("seed pending action");
    db::project_store::update_agent_run_status(
        &repo_root,
        &project_id,
        run_id,
        db::project_store::AgentRunStatus::Paused,
        None,
        "2026-05-04T10:00:01Z",
    )
    .expect("pause run");

    let subscription = xero_desktop_lib::runtime::subscribe_agent_events(&project_id, run_id);
    let rejected = <DesktopAgentCoreRuntime as AgentRuntimeFacade>::reject_action(
        &runtime,
        DesktopRejectActionRequest {
            repo_root: repo_root.clone(),
            request: ApprovalDecisionRequest {
                project_id: project_id.clone(),
                run_id: run_id.into(),
                action_id: "tool-call-denied".into(),
                response: Some("Do not run this tool.".into()),
            },
        },
    )
    .expect("reject pending action");

    assert_eq!(
        rejected.run.status,
        db::project_store::AgentRunStatus::Failed
    );
    let action = rejected
        .action_requests
        .iter()
        .find(|action| action.action_id == "tool-call-denied")
        .expect("rejected action persisted");
    assert_eq!(action.status, "rejected");
    assert_eq!(action.response.as_deref(), Some("Do not run this tool."));

    let mut saw_stream_rejection = false;
    for _ in 0..3 {
        let event = subscription
            .recv_timeout(Duration::from_secs(1))
            .expect("rejection event streamed");
        let payload: serde_json::Value =
            serde_json::from_str(&event.payload_json).expect("event payload json");
        if event.event_kind == db::project_store::AgentRunEventKind::PolicyDecision
            && payload.get("decision").and_then(serde_json::Value::as_str) == Some("rejected")
        {
            saw_stream_rejection = true;
            break;
        }
    }
    assert!(saw_stream_rejection);

    let trace = <DesktopAgentCoreRuntime as AgentRuntimeFacade>::export_trace(
        &runtime,
        xero_desktop_lib::runtime::DesktopExportTraceRequest {
            repo_root,
            project_id,
            run_id: run_id.into(),
        },
    )
    .expect("export rejection trace");
    assert_eq!(trace.snapshot.status, xero_agent_core::RunStatus::Failed);
    assert!(trace.snapshot.events.iter().any(|event| {
        event.event_kind == xero_agent_core::RuntimeEventKind::PolicyDecision
            && event
                .payload
                .get("actionId")
                .and_then(serde_json::Value::as_str)
                == Some("tool-call-denied")
            && event.trace.is_valid()
    }));
}

#[test]
fn desktop_facade_fork_session_copies_lineage_and_context_manifests() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let state = app.state::<DesktopState>();
    let (project_id, repo_root) = seed_project(&root, &app);
    let source_run_id = "desktop-facade-fork-source-1";
    let tool_runtime =
        AutonomousToolRuntime::for_project(&app.handle().clone(), state.inner(), &project_id)
            .expect("build autonomous tool runtime");
    let source = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: source_run_id.into(),
        prompt: "Create durable context for a fork.\ntool:read src/tracked.txt".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("source owned-agent run");
    let source_manifests = db::project_store::list_agent_context_manifests_for_run(
        &repo_root,
        &project_id,
        source_run_id,
    )
    .expect("source context manifests");
    assert!(!source_manifests.is_empty());

    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());
    let forked = <DesktopAgentCoreRuntime as AgentRuntimeFacade>::fork_session(
        &runtime,
        DesktopForkSessionRequest {
            repo_root: repo_root.clone(),
            request: ForkSessionRequest {
                project_id: project_id.clone(),
                source_agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
                target_agent_session_id: "agent-session-facade-fork-target".into(),
            },
            source_run_id: Some(source_run_id.into()),
            title: Some("Facade fork".into()),
            selected: true,
        },
    )
    .expect("fork desktop session");

    assert_eq!(
        forked.run.agent_session_id,
        "agent-session-facade-fork-target"
    );
    assert_eq!(forked.run.parent_run_id.as_deref(), Some(source_run_id));
    assert_eq!(
        forked.run.parent_trace_id.as_deref(),
        Some(source.run.trace_id.as_str())
    );
    assert!(forked.messages.iter().any(|message| {
        message
            .content
            .contains("Create durable context for a fork")
    }));

    let fork_session = db::project_store::get_agent_session(
        &repo_root,
        &project_id,
        "agent-session-facade-fork-target",
    )
    .expect("load forked session")
    .expect("forked session exists");
    let lineage = fork_session.lineage.expect("fork lineage");
    assert_eq!(lineage.source_run_id.as_deref(), Some(source_run_id));
    assert_eq!(lineage.replay_run_id, forked.run.run_id);

    let fork_manifests = db::project_store::list_agent_context_manifests_for_run(
        &repo_root,
        &project_id,
        &forked.run.run_id,
    )
    .expect("forked context manifests");
    assert_eq!(fork_manifests.len(), source_manifests.len());
    assert!(fork_manifests.iter().all(|manifest| {
        manifest.agent_session_id == "agent-session-facade-fork-target"
            && manifest.run_id.as_deref() == Some(forked.run.run_id.as_str())
            && manifest
                .manifest
                .get("lineage")
                .and_then(|lineage| lineage.get("sourceRunId"))
                .and_then(serde_json::Value::as_str)
                == Some(source_run_id)
    }));

    let trace = <DesktopAgentCoreRuntime as AgentRuntimeFacade>::export_trace(
        &runtime,
        xero_desktop_lib::runtime::DesktopExportTraceRequest {
            repo_root,
            project_id,
            run_id: forked.run.run_id.clone(),
        },
    )
    .expect("export fork trace");
    assert_eq!(trace.snapshot.context_manifests.len(), fork_manifests.len());
    assert!(trace.snapshot.events.iter().any(|event| {
        event.event_kind == xero_agent_core::RuntimeEventKind::StateTransition
            && event
                .payload
                .get("kind")
                .and_then(serde_json::Value::as_str)
                == Some("session_forked")
            && event.trace.is_valid()
    }));
}

#[test]
fn provider_preflight_manifest_binding() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let preflight = live_provider_preflight("fake_provider", "test-model");
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
        run_id: "provider-preflight-manifest-binding".into(),
        prompt: "Bind provider preflight to the context manifest.\ntool:read src/tracked.txt"
            .into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: Some(preflight.clone()),
    })
    .expect("owned agent run with admitted provider preflight");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed,
        "last error: {:?}",
        snapshot.run.last_error
    );
    let manifests = db::project_store::list_agent_context_manifests_for_run(
        &repo_root,
        &project_id,
        "provider-preflight-manifest-binding",
    )
    .expect("context manifests");
    assert!(!manifests.is_empty());
    let admitted_preflight = serde_json::to_value(&preflight).expect("serialize preflight");

    assert!(manifests.iter().all(|manifest| {
        manifest.manifest.get("providerPreflight") == Some(&admitted_preflight)
            && manifest
                .manifest
                .get("admittedProviderPreflightHash")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|hash| !hash.is_empty())
    }));
}

#[test]
fn canonical_trace_passes_production_gates() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let provider_id = "openai_api";
    let model_id = "test-model";
    let preflight = live_provider_preflight(provider_id, model_id);
    let server = MockOpenAiCompatibleSseServer::start(vec![
        openai_chat_sse(vec![json!({
            "choices": [{
                "delta": {
                    "tool_calls": [{
                        "index": 0,
                        "id": "call-read",
                        "function": {
                            "name": "read",
                            "arguments": "{\"path\":\"src/tracked.txt\"}"
                        }
                    }]
                }
            }]
        })]),
        openai_chat_sse(vec![json!({
            "choices": [{
                "delta": {
                    "content": "Read completed through Tool Registry V2."
                }
            }]
        })]),
    ]);
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
        run_id: "canonical-trace-production-gates".into(),
        prompt: "Read the tracked file.\ntool:read src/tracked.txt".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::OpenAiCompatible(OpenAiCompatibleProviderConfig {
            provider_id: provider_id.into(),
            model_id: model_id.into(),
            base_url: server.base_url.clone(),
            api_key: Some("test-key".into()),
            api_version: None,
            timeout_ms: 2_000,
        }),
        provider_preflight: Some(preflight),
    })
    .expect("real-provider mock run should complete");
    server.join();

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed,
        "last error: {:?}",
        snapshot.run.last_error
    );

    let runtime =
        DesktopAgentCoreRuntime::new(app.state::<DesktopState>().agent_run_supervisor().clone());
    let trace = <DesktopAgentCoreRuntime as AgentRuntimeFacade>::export_trace(
        &runtime,
        xero_desktop_lib::runtime::DesktopExportTraceRequest {
            repo_root,
            project_id,
            run_id: "canonical-trace-production-gates".into(),
        },
    )
    .expect("export canonical trace");
    let canonical = trace.canonical_snapshot().expect("canonical trace");
    let support_bundle = trace.redacted_support_bundle().expect("support bundle");
    let readiness = canonical.production_readiness_report(production_readiness_focused_tests(
        ProductionReadinessFocusedTestStatus::Passed,
    ));

    assert!(
        canonical.quality_gates.passed,
        "canonical trace quality gates should pass: {:?}",
        canonical
            .quality_gates
            .gates
            .iter()
            .filter(|gate| gate.status == xero_agent_core::TraceQualityGateStatus::Fail)
            .collect::<Vec<_>>()
    );
    assert_eq!(readiness.status, ProductionReadinessStatus::Ready);
    assert_eq!(canonical.trace_id, support_bundle.trace_id);
    assert_eq!(
        canonical
            .timeline
            .items
            .iter()
            .map(|item| item.event_id)
            .collect::<Vec<_>>(),
        support_bundle
            .timeline
            .items
            .iter()
            .map(|item| item.event_id)
            .collect::<Vec<_>>()
    );
}

#[test]
fn desktop_facade_compact_session_persists_artifact_and_trace() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let state = app.state::<DesktopState>();
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "desktop-facade-compact-1";
    let tool_runtime =
        AutonomousToolRuntime::for_project(&app.handle().clone(), state.inner(), &project_id)
            .expect("build autonomous tool runtime");
    run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: run_id.into(),
        prompt: "Create compactable history.\ntool:read src/tracked.txt".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("source owned-agent run");
    let manifest_count_before =
        db::project_store::list_agent_context_manifests_for_run(&repo_root, &project_id, run_id)
            .expect("context manifests before compact")
            .len();

    let runtime = DesktopAgentCoreRuntime::new(state.inner().agent_run_supervisor().clone());
    let compacted = <DesktopAgentCoreRuntime as AgentRuntimeFacade>::compact_session(
        &runtime,
        DesktopCompactSessionRequest {
            repo_root: repo_root.clone(),
            request: CompactSessionRequest {
                project_id: project_id.clone(),
                agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
                reason: "manual_compact_requested".into(),
            },
            run_id: Some(run_id.into()),
            raw_tail_message_count: Some(2),
            trigger: db::project_store::AgentCompactionTrigger::Manual,
            provider_config: AgentProviderConfig::Fake,
        },
    )
    .expect("compact desktop session");

    assert_eq!(
        compacted.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    let active_compaction = db::project_store::load_active_agent_compaction(
        &repo_root,
        &project_id,
        db::project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect("load active compaction")
    .expect("active compaction");
    assert_eq!(active_compaction.raw_tail_message_count, 2);
    assert_eq!(active_compaction.policy_reason, "manual_compact_requested");

    let manifests =
        db::project_store::list_agent_context_manifests_for_run(&repo_root, &project_id, run_id)
            .expect("context manifests after compact");
    assert_eq!(manifests.len(), manifest_count_before + 1);
    let artifact = manifests
        .iter()
        .find(|manifest| {
            manifest.compaction_id.as_deref() == Some(active_compaction.compaction_id.as_str())
        })
        .expect("compaction context artifact");
    assert_eq!(
        artifact
            .manifest
            .get("rawTailMessageCount")
            .and_then(serde_json::Value::as_u64),
        Some(2)
    );

    let trace = <DesktopAgentCoreRuntime as AgentRuntimeFacade>::export_trace(
        &runtime,
        xero_desktop_lib::runtime::DesktopExportTraceRequest {
            repo_root,
            project_id,
            run_id: run_id.into(),
        },
    )
    .expect("export compaction trace");
    assert!(trace.snapshot.context_manifests.iter().any(|manifest| {
        manifest.manifest_id == artifact.manifest_id && manifest.trace.is_valid()
    }));
    assert!(trace.snapshot.events.iter().any(|event| {
        event.event_kind == xero_agent_core::RuntimeEventKind::PolicyDecision
            && event
                .payload
                .get("kind")
                .and_then(serde_json::Value::as_str)
                == Some("session_compaction")
            && event.trace.is_valid()
    }));
}

#[test]
fn core_runtime_contract_inventory_covers_store_modes_tools_and_manifest_metadata() {
    let root = TempDir::new().unwrap();
    let contract = export_harness_contract(root.path(), Default::default())
        .expect("export harness contract inventory");

    assert!(contract
        .tool_registry_snapshots
        .iter()
        .flat_map(|snapshot| snapshot.descriptors_v2.iter())
        .any(|descriptor| descriptor.name == "project_context_search"
            && descriptor.input_schema["properties"]["action"]["enum"]
                .as_array()
                .expect("project_context_search actions")
                .contains(&json!("search_project_records"))));
    assert!(contract
        .tool_registry_snapshots
        .iter()
        .flat_map(|snapshot| snapshot.descriptors_v2.iter())
        .any(|descriptor| descriptor.name == "project_context_get"
            && descriptor.input_schema["properties"]["action"]["enum"]
                .as_array()
                .expect("project_context_get actions")
                .contains(&json!("get_project_record"))));
    assert!(contract
        .tool_registry_snapshots
        .iter()
        .any(|snapshot| !snapshot.descriptors_v2.is_empty()
            && !snapshot.descriptors_v2_sha256.is_empty()));

    let real_store = RuntimeStoreDescriptor::app_data_project_state(
        "project-contract",
        root.path()
            .join("app-data")
            .join("projects")
            .join("project-contract")
            .join("state.db"),
    );
    let real_contract = xero_agent_core::ProductionRuntimeContract::real_provider(
        "desktop_owned_agent",
        "project-contract",
        "openai_api",
        "gpt-5.4",
        real_store,
    );
    assert_eq!(
        real_contract.execution_mode,
        RuntimeExecutionMode::ProductionRealProvider
    );
    assert_eq!(
        real_contract.store.kind,
        RuntimeStoreKind::AppDataProjectState
    );
    xero_agent_core::validate_production_runtime_contract(&real_contract)
        .expect("real provider contract requires app-data state.db");

    let harness_contract = xero_agent_core::ProductionRuntimeContract::fake_provider_harness(
        "headless_harness",
        "project-contract",
        "fake-model",
        RuntimeStoreDescriptor::file_backed_headless_json(
            "project-contract",
            root.path().join("agent-core-runs.json"),
        ),
    );
    assert_eq!(
        harness_contract.execution_mode,
        RuntimeExecutionMode::HarnessFakeProvider
    );
    assert_eq!(
        harness_contract.store.kind,
        RuntimeStoreKind::FileBackedHeadlessJson
    );
    xero_agent_core::validate_production_runtime_contract(&harness_contract)
        .expect("fake harness may use file-backed harness storage");

    let manifest = json!({
        "retrieval": {
            "deliveryModel": "tool_mediated",
            "rawContextInjected": false,
            "queryIds": ["context-retrieval-contract"],
            "resultIds": ["context-retrieval-contract-result-1"]
        }
    });
    assert_eq!(manifest["retrieval"]["deliveryModel"], "tool_mediated");
    assert_eq!(manifest["retrieval"]["rawContextInjected"], false);
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
    let mut expected_names = BTreeSet::from([
        "read",
        "search",
        "find",
        "git_status",
        "git_diff",
        "tool_access",
        "project_context_search",
        "project_context_get",
        "project_context_record",
        "project_context_update",
        "project_context_refresh",
        "workspace_index",
        "agent_coordination",
        "edit",
        "write",
        "patch",
        "delete",
        "rename",
        "mkdir",
        "list",
        "file_hash",
        "command_probe",
        "command_verify",
        "command_run",
        "command_session",
        "process_manager",
        "mcp_list",
        "mcp_read_resource",
        "mcp_get_prompt",
        "mcp_call_tool",
        "subagent",
        "todo",
        "notebook_edit",
        "code_intel",
        "lsp",
        "tool_search",
        "web_search",
        "web_fetch",
        "browser_observe",
        "browser_control",
        "emulator",
        "environment_context",
        "system_diagnostics_observe",
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
    if cfg!(target_os = "macos") {
        expected_names.insert("macos_automation");
        expected_names.insert("system_diagnostics_privileged");
    }
    if cfg!(target_os = "windows") {
        expected_names.insert("powershell");
    }
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
    let project_context_search = registry
        .descriptor("project_context_search")
        .expect("project context search descriptor");
    assert_eq!(
        project_context_search.input_schema["required"],
        json!(["action"])
    );
    assert!(
        project_context_search.input_schema["properties"]["action"]["enum"]
            .as_array()
            .expect("project context action enum")
            .contains(&json!("search_project_records"))
    );
    let project_context_record = registry
        .descriptor("project_context_record")
        .expect("project context record descriptor");
    assert!(
        project_context_record.input_schema["properties"]["action"]["enum"]
            .as_array()
            .expect("project context action enum")
            .contains(&json!("propose_record_candidate"))
    );

    let process_manager = registry
        .descriptor("process_manager")
        .expect("process manager descriptor");
    assert_eq!(process_manager.input_schema["required"], json!(["action"]));
    assert!(process_manager.input_schema["properties"]["action"]["enum"]
        .as_array()
        .expect("process manager action enum")
        .contains(&json!("async_start")));
    assert!(process_manager.description.contains("phase 5"));

    if cfg!(target_os = "macos") {
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
    } else {
        assert!(registry.descriptor("macos_automation").is_none());
    }

    assert!(registry.descriptor("browser_observe").is_some());
    assert!(registry.descriptor("browser_control").is_some());
    assert!(registry.descriptor("mcp_list").is_some());
    assert!(registry.descriptor("mcp_call_tool").is_some());
    assert!(registry.descriptor("subagent").is_some());
    assert!(registry.descriptor("todo").is_some());
    assert!(registry.descriptor("notebook_edit").is_some());
    assert!(registry.descriptor("code_intel").is_some());
    assert!(registry.descriptor("lsp").is_some());
    assert_eq!(
        registry.descriptor("powershell").is_some(),
        cfg!(target_os = "windows")
    );
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
fn tool_registry_v2_validates_every_builtin_descriptor_sample() {
    fn sample_for_schema(schema: &serde_json::Value) -> serde_json::Value {
        if let Some(branch) = schema
            .get("oneOf")
            .and_then(serde_json::Value::as_array)
            .and_then(|branches| branches.first())
        {
            return sample_for_schema(branch);
        }
        match schema
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("object")
        {
            "array" => json!([sample_for_schema(
                schema
                    .get("items")
                    .unwrap_or(&serde_json::Value::String("sample".into()))
            )]),
            "boolean" => json!(true),
            "integer" => json!(schema
                .get("minimum")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(1)
                .max(1)),
            "number" => json!(1.0),
            "object" => {
                let mut object = serde_json::Map::new();
                let properties = schema
                    .get("properties")
                    .and_then(serde_json::Value::as_object);
                if let (Some(required), Some(properties)) = (
                    schema.get("required").and_then(serde_json::Value::as_array),
                    properties,
                ) {
                    for field in required.iter().filter_map(serde_json::Value::as_str) {
                        if let Some(field_schema) = properties.get(field) {
                            object.insert(field.to_string(), sample_for_schema(field_schema));
                        }
                    }
                }
                serde_json::Value::Object(object)
            }
            "string" => schema
                .get("enum")
                .and_then(serde_json::Value::as_array)
                .and_then(|values| values.first())
                .cloned()
                .unwrap_or_else(|| json!("sample")),
            _ => serde_json::Value::Null,
        }
    }

    fn invalid_for_schema(schema: &serde_json::Value) -> serde_json::Value {
        if schema.get("oneOf").is_some() {
            return json!({"xeroUnexpected": true});
        }
        let mut sample = sample_for_schema(schema);
        if let Some(object) = sample.as_object_mut() {
            object.insert("xeroUnexpected".into(), json!(true));
            return sample;
        }
        json!(null)
    }

    let registry = ToolRegistry::builtin_with_options(ToolRegistryOptions {
        runtime_agent_id: RuntimeAgentIdDto::Engineer,
        skill_tool_enabled: true,
        ..ToolRegistryOptions::default()
    });
    let mut registry_v2 = ToolRegistryV2::new();
    let descriptors = registry.descriptors_v2();
    for descriptor in descriptors.iter().cloned() {
        let mut descriptor = descriptor;
        descriptor.approval_requirement = ToolApprovalRequirement::Never;
        descriptor.sandbox_requirement = ToolSandboxRequirement::None;
        registry_v2
            .register(StaticToolHandler::new(descriptor, |_context, _call| {
                Ok(ToolHandlerOutput::new("ok", json!({ "accepted": true })))
            }))
            .expect("register builtin descriptor");
    }

    for descriptor in &descriptors {
        let valid = registry_v2.dispatch_call(
            ToolCallInput {
                tool_call_id: format!("valid-{}", descriptor.name),
                tool_name: descriptor.name.clone(),
                input: sample_for_schema(&descriptor.input_schema),
            },
            &mut xero_agent_core::ToolBudgetTracker::new(ToolBudget::default()),
            &ToolDispatchConfig::default(),
        );
        assert!(
            matches!(valid, xero_agent_core::ToolDispatchOutcome::Succeeded(_)),
            "valid sample failed for {}: {valid:#?}",
            descriptor.name
        );

        let invalid = registry_v2.dispatch_call(
            ToolCallInput {
                tool_call_id: format!("invalid-{}", descriptor.name),
                tool_name: descriptor.name.clone(),
                input: invalid_for_schema(&descriptor.input_schema),
            },
            &mut xero_agent_core::ToolBudgetTracker::new(ToolBudget::default()),
            &ToolDispatchConfig::default(),
        );
        assert_eq!(
            invalid.failure().unwrap().error.category,
            ToolErrorCategory::InvalidInput,
            "invalid sample should fail for {}",
            descriptor.name
        );
    }
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
    assert!(read_only_names.contains("project_context_search"));
    assert!(read_only_names.contains("project_context_get"));
    assert!(read_only_names.contains("tool_search"));
    assert!(read_only_names.contains("todo"));
    assert!(read_only_names.contains("git_diff"));
    assert!(!read_only_names.contains("write"));
    assert!(!read_only_names.contains("command_run"));
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
    assert!(implementation_names.contains("command_probe"));
    assert!(implementation_names.contains("command_verify"));
    assert!(!implementation_names.contains("command_run"));
    assert!(!implementation_names.contains("command_session"));
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
    assert_eq!(
        macos.descriptor_names().contains("macos_automation"),
        cfg!(target_os = "macos")
    );

    let audit = ToolRegistry::for_prompt(
        temp.path(),
        "Thoroughly audit the harness and verify it is production grade.",
        &controls,
    );
    let audit_names = audit.descriptor_names();
    assert!(audit_names.contains("read"));
    assert!(audit_names.contains("git_diff"));
    assert!(audit_names.contains("command_verify"));

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
    assert!(priority_names.contains("mcp_list"));
    assert!(!priority_names.contains("mcp_call_tool"));
    assert!(priority_names.contains("subagent"));
    assert!(priority_names.contains("todo"));
    assert!(priority_names.contains("tool_search"));
    assert!(priority_names.contains("code_intel"));
    assert!(priority_names.contains("lsp"));
    assert!(priority_names.contains("notebook_edit"));
    assert_eq!(
        priority_names.contains("powershell"),
        cfg!(target_os = "windows")
    );

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
    assert!(default_browser_names.contains("browser_observe"));
    assert!(default_browser_names.contains("browser_control"));
    assert!(!default_browser_names.contains("macos_automation"));

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
    assert!(in_app_browser_names.contains("browser_observe"));
    assert!(in_app_browser_names.contains("browser_control"));
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
    assert!(!native_browser_names.contains("browser_observe"));
    assert!(!native_browser_names.contains("browser_control"));
    assert_eq!(
        native_browser_names.contains("macos_automation"),
        cfg!(target_os = "macos")
    );
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
        provider_preflight: None,
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
    assert!(tool_names.contains(&"command_probe"));
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

    let connection =
        Connection::open(db::database_path_for_repo(&repo_root)).expect("open project db");
    let exact_commit_count: i64 = connection
        .query_row(
            r#"
            SELECT COUNT(*)
            FROM code_commits
            JOIN code_change_groups
              ON code_change_groups.project_id = code_commits.project_id
             AND code_change_groups.change_group_id = code_commits.change_group_id
            WHERE code_commits.project_id = ?1
              AND code_commits.run_id = 'owned-run-file-tools-1'
              AND code_change_groups.change_kind = 'file_tool'
            "#,
            params![project_id.as_str()],
            |row| row.get(0),
        )
        .expect("count exact file tool commits");
    assert_eq!(exact_commit_count, 5);
    let workspace_head = db::project_store::read_code_workspace_head(&repo_root, &project_id)
        .expect("read code workspace head")
        .expect("code workspace head");
    assert_eq!(workspace_head.workspace_epoch, 5);
    assert!(workspace_head
        .head_id
        .as_deref()
        .is_some_and(|head_id| head_id.starts_with("code-commit-")));
    let patch_operations = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT code_patch_files.operation
                FROM code_patch_files
                JOIN code_commits
                  ON code_commits.project_id = code_patch_files.project_id
                 AND code_commits.patchset_id = code_patch_files.patchset_id
                JOIN code_change_groups
                  ON code_change_groups.project_id = code_commits.project_id
                 AND code_change_groups.change_group_id = code_commits.change_group_id
                WHERE code_commits.project_id = ?1
                  AND code_commits.run_id = 'owned-run-file-tools-1'
                  AND code_change_groups.change_kind = 'file_tool'
                ORDER BY code_commits.workspace_epoch ASC, code_patch_files.file_index ASC
                "#,
            )
            .expect("prepare patch operation query");
        statement
            .query_map(params![project_id.as_str()], |row| row.get::<_, String>(0))
            .expect("query patch operations")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect patch operations")
    };
    assert!(patch_operations
        .iter()
        .any(|operation| operation == "modify"));
    assert!(patch_operations
        .iter()
        .any(|operation| operation == "create"));
    assert!(patch_operations
        .iter()
        .any(|operation| operation == "rename"));
    assert!(patch_operations
        .iter()
        .any(|operation| operation == "delete"));
    let mkdir_kind: String = connection
        .query_row(
            r#"
            SELECT after_file_kind
            FROM code_patch_files
            JOIN code_commits
              ON code_commits.project_id = code_patch_files.project_id
             AND code_commits.patchset_id = code_patch_files.patchset_id
            WHERE code_commits.project_id = ?1
              AND code_commits.run_id = 'owned-run-file-tools-1'
              AND code_patch_files.path_after = 'generated'
            "#,
            params![project_id.as_str()],
            |row| row.get(0),
        )
        .expect("mkdir patch file kind");
    assert_eq!(mkdir_kind, "directory");
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
        provider_preflight: None,
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
    assert!(tool_names.contains(&"mcp_list"));
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
    assert_eq!(snapshot.run.lineage_kind, "top_level");
    assert_eq!(child_snapshot.run.lineage_kind, "subagent_child");
    assert_eq!(
        child_snapshot.run.agent_session_id,
        snapshot.run.agent_session_id
    );
    assert_eq!(
        child_snapshot.run.parent_run_id.as_deref(),
        Some(snapshot.run.run_id.as_str())
    );
    assert_eq!(
        child_snapshot.run.parent_trace_id.as_deref(),
        Some(snapshot.run.trace_id.as_str())
    );
    assert_eq!(
        child_snapshot.run.parent_subagent_id.as_deref(),
        Some("subagent-1")
    );
    assert_eq!(
        child_snapshot.run.subagent_role.as_deref(),
        Some("researcher")
    );
    assert_eq!(snapshot.run.trace_id.len(), 32);
    assert_eq!(child_snapshot.run.trace_id.len(), 32);
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
        provider_preflight: None,
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
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::EnvironmentLifecycleUpdate));
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::ToolRegistrySnapshot));
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::ToolStarted));
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::ToolCompleted));
    assert!(event_kinds.contains(&db::project_store::AgentRunEventKind::RunCompleted));
    let ready_lifecycle_event = snapshot
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::EnvironmentLifecycleUpdate
                && serde_json::from_str::<serde_json::Value>(&event.payload_json)
                    .ok()
                    .and_then(|payload| payload["state"].as_str().map(ToOwned::to_owned))
                    .as_deref()
                    == Some("ready")
        })
        .expect("ready environment lifecycle event");
    let registry_event = snapshot
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::ToolRegistrySnapshot
        })
        .expect("tool registry event");
    assert!(
        ready_lifecycle_event.id < registry_event.id,
        "environment readiness must be persisted before provider turn setup"
    );
    let lifecycle_snapshot = db::project_store::load_agent_environment_lifecycle_snapshot(
        &repo_root,
        &project_id,
        &snapshot.run.run_id,
    )
    .expect("load environment lifecycle snapshot")
    .expect("environment lifecycle snapshot should persist beside the run");
    assert_eq!(lifecycle_snapshot.state, "ready");
    let lifecycle_health: serde_json::Value =
        serde_json::from_str(&lifecycle_snapshot.health_checks_json).expect("health checks json");
    assert!(lifecycle_health
        .as_array()
        .expect("health check array")
        .iter()
        .any(|check| check["kind"] == "filesystem_accessible" && check["status"] == "passed"));
    let trace = DesktopAgentCoreRuntime::new(AgentRunSupervisor::default())
        .export_trace(
            repo_root.clone(),
            project_id.clone(),
            snapshot.run.run_id.clone(),
        )
        .expect("export runtime trace");
    assert!(trace
        .snapshot
        .events
        .iter()
        .any(|event| event.event_kind == RuntimeEventKind::EnvironmentLifecycleUpdate));
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
    assert_eq!(
        registry_payload["exposurePlan"]["schema"],
        "xero.tool_exposure_plan.v1"
    );
    assert!(registry_payload["exposurePlan"]["entries"]
        .as_array()
        .expect("exposure entries")
        .iter()
        .any(|entry| entry["toolName"] == "read"
            && entry["reasons"]
                .as_array()
                .expect("read exposure reasons")
                .iter()
                .any(|reason| reason["source"] == "user_explicit_tool_marker")));

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
fn workspace_index_required_blocks_lifecycle() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    let empty_required = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-workspace-index-empty".into(),
        prompt: "Find related tests for the runtime lifecycle using semantic workspace search."
            .into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime: AutonomousToolRuntime::for_project(
            &app.handle().clone(),
            app.state::<DesktopState>().inner(),
            &project_id,
        )
        .expect("build empty-index runtime"),
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("empty required run should persist blocked snapshot");
    assert_eq!(
        empty_required.run.status,
        db::project_store::AgentRunStatus::Failed
    );
    assert!(!empty_required.events.iter().any(
        |event| event.event_kind == db::project_store::AgentRunEventKind::ToolRegistrySnapshot
    ));
    let empty_health =
        semantic_lifecycle_health_check(&repo_root, &project_id, &empty_required.run.run_id);
    assert_eq!(empty_health["status"], "failed");
    assert_eq!(
        empty_health["diagnostic"]["code"],
        "agent_environment_workspace_index_empty"
    );

    let optional = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-workspace-index-optional".into(),
        prompt: "Summarize the current project instructions.".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime: AutonomousToolRuntime::for_project(
            &app.handle().clone(),
            app.state::<DesktopState>().inner(),
            &project_id,
        )
        .expect("build optional-index runtime"),
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("optional empty index run should proceed");
    assert_eq!(
        optional.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    let optional_health =
        semantic_lifecycle_health_check(&repo_root, &project_id, &optional.run.run_id);
    assert_eq!(optional_health["status"], "warning");

    seed_workspace_index_ready(&repo_root, &project_id);
    fs::write(
        repo_root.join("src").join("indexed.rs"),
        "pub fn semantic_workspace_index_stale() -> bool { true }\n",
    )
    .expect("make workspace index stale");
    let stale_required = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-workspace-index-stale".into(),
        prompt: "Use semantic workspace search to find related tests for lifecycle.".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime: AutonomousToolRuntime::for_project(
            &app.handle().clone(),
            app.state::<DesktopState>().inner(),
            &project_id,
        )
        .expect("build stale-index runtime"),
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("stale required run should persist blocked snapshot");
    assert_eq!(
        stale_required.run.status,
        db::project_store::AgentRunStatus::Failed
    );
    let stale_health =
        semantic_lifecycle_health_check(&repo_root, &project_id, &stale_required.run.run_id);
    assert_eq!(
        stale_health["diagnostic"]["code"],
        "agent_environment_workspace_index_stale"
    );

    seed_workspace_index_ready(&repo_root, &project_id);
    let ready_required = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-workspace-index-ready".into(),
        prompt: "Use semantic workspace search to find related tests for lifecycle.".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime: AutonomousToolRuntime::for_project(
            &app.handle().clone(),
            app.state::<DesktopState>().inner(),
            &project_id,
        )
        .expect("build ready-index runtime"),
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("ready required run should proceed");
    assert_eq!(
        ready_required.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    let ready_health =
        semantic_lifecycle_health_check(&repo_root, &project_id, &ready_required.run.run_id);
    assert_eq!(ready_health["status"], "passed");
    let ready_lifecycle_event = ready_required
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::EnvironmentLifecycleUpdate
                && serde_json::from_str::<serde_json::Value>(&event.payload_json)
                    .ok()
                    .and_then(|payload| payload["state"].as_str().map(ToOwned::to_owned))
                    .as_deref()
                    == Some("ready")
        })
        .expect("ready lifecycle event");
    let registry_event = ready_required
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::ToolRegistrySnapshot
        })
        .expect("tool registry event after readiness");
    assert!(ready_lifecycle_event.id < registry_event.id);
}

#[test]
fn owned_agent_provider_loop_dispatches_read_only_batches_through_tool_registry_v2() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::write(repo_root.join("src").join("second.txt"), "delta\nepsilon\n")
        .expect("seed second file");
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build desktop autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-tool-registry-v2-read-batch".into(),
        prompt: [
            "Read both files in one provider turn.",
            "tool:read src/tracked.txt",
            "tool:read src/second.txt",
        ]
        .join("\n"),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("read-only batch run succeeds");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert_eq!(
        snapshot
            .tool_calls
            .iter()
            .filter(|call| {
                call.tool_name == "read"
                    && call.state == db::project_store::AgentToolCallState::Succeeded
            })
            .count(),
        2
    );

    let registry_payload: serde_json::Value = serde_json::from_str(
        &snapshot
            .events
            .iter()
            .find(|event| {
                event.event_kind == db::project_store::AgentRunEventKind::ToolRegistrySnapshot
            })
            .expect("tool registry snapshot")
            .payload_json,
    )
    .expect("registry payload json");
    assert_eq!(registry_payload["executionRegistry"], "tool_registry_v2");
    assert!(registry_payload["descriptorsV2"]
        .as_array()
        .expect("v2 descriptors")
        .iter()
        .any(|descriptor| descriptor["name"] == "read"
            && descriptor["mutability"] == "read_only"
            && descriptor["sandboxRequirement"] == "read_only"));

    let read_completed_payloads = snapshot
        .events
        .iter()
        .filter(|event| event.event_kind == db::project_store::AgentRunEventKind::ToolCompleted)
        .filter_map(|event| serde_json::from_str::<serde_json::Value>(&event.payload_json).ok())
        .filter(|payload| payload["toolName"] == "read" && payload["ok"] == true)
        .collect::<Vec<_>>();
    assert_eq!(read_completed_payloads.len(), 2);
    assert!(read_completed_payloads.iter().all(|payload| {
        payload["dispatch"]["registryVersion"] == "tool_registry_v2"
            && payload["dispatch"]["groupMode"] == "parallel_read_only"
            && payload["dispatch"]["truncation"]["wasTruncated"] == false
            && payload["dispatch"]["sandbox"]["profile"] == "read_only"
            && payload["dispatch"]["budget"]["maxToolCallsPerTurn"].as_u64() == Some(128)
    }));
    assert_eq!(
        snapshot
            .messages
            .iter()
            .filter(|message| {
                message.role == db::project_store::AgentMessageRole::Tool
                    && message.content.contains("\"toolName\":\"read\"")
            })
            .count(),
        2
    );
}

#[test]
fn tool_registry_v2_enforces_sandbox_denial() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::create_dir_all(repo_root.join(".xero")).expect("seed legacy state directory");
    let tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build desktop autonomous tool runtime");

    let snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-tool-registry-v2-sandbox-denial".into(),
        prompt: "Try to write legacy state.\ntool:write .xero/blocked.txt forbidden\n".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("sandbox denial should persist a failed tool outcome");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Paused
    );
    assert!(
        !repo_root.join(".xero").join("blocked.txt").exists(),
        "sandbox denial must prevent handler execution and file writes"
    );
    assert!(snapshot
        .file_changes
        .iter()
        .all(|change| change.path != ".xero/blocked.txt"));

    let write_call = snapshot
        .tool_calls
        .iter()
        .find(|tool_call| tool_call.tool_name == "write")
        .expect("write tool call");
    assert_eq!(
        write_call.state,
        db::project_store::AgentToolCallState::Failed
    );
    assert!(write_call.error.as_ref().is_some_and(|error| {
        error.code == "agent_sandbox_path_denied" && error.message.contains(".xero/")
    }));

    let completion_payload = snapshot
        .events
        .iter()
        .filter(|event| event.event_kind == db::project_store::AgentRunEventKind::ToolCompleted)
        .filter_map(|event| serde_json::from_str::<serde_json::Value>(&event.payload_json).ok())
        .find(|payload| payload["toolName"] == "write")
        .expect("failed write completion payload");
    assert_eq!(completion_payload["ok"], false);
    assert_eq!(
        completion_payload["dispatch"]["typedErrorCategory"],
        "sandbox_denied"
    );
    assert_eq!(
        completion_payload["dispatch"]["sandbox"]["exitClassification"],
        "denied_by_sandbox"
    );
    assert!(completion_payload["dispatch"]["sandbox"]["blockedReason"]
        .as_str()
        .is_some_and(|reason| reason.contains(".xero/")));
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
        provider_preflight: None,
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
fn owned_agent_queues_user_messages_until_environment_ready() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "owned-run-environment-queue-1";
    let request = OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: run_id.into(),
        prompt: "Start the run after lifecycle.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: AutonomousToolRuntime::for_project(
            &app.handle().clone(),
            app.state::<DesktopState>().inner(),
            &project_id,
        )
        .expect("build autonomous tool runtime"),
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    };
    create_owned_agent_run(&request).expect("create owned agent run");
    db::project_store::upsert_agent_environment_lifecycle_snapshot(
        &repo_root,
        &db::project_store::NewAgentEnvironmentLifecycleSnapshotRecord {
            project_id: project_id.clone(),
            run_id: run_id.into(),
            environment_id: format!("env-{project_id}-{run_id}"),
            state: "preparing_repository".into(),
            previous_state: Some("waiting_for_sandbox".into()),
            pending_message_count: 0,
            health_checks_json: "[]".into(),
            setup_steps_json: "[]".into(),
            diagnostic_json: None,
            snapshot_json: json!({
                "schema": "xero.environment_lifecycle.v1",
                "environmentId": format!("env-{project_id}-{run_id}"),
                "state": "preparing_repository",
                "previousState": "waiting_for_sandbox",
                "pendingMessageCount": 0,
                "healthChecks": [],
                "setupSteps": []
            })
            .to_string(),
            updated_at: "2026-05-04T12:00:00Z".into(),
        },
    )
    .expect("seed not-ready lifecycle snapshot");

    let queued = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: run_id.into(),
        prompt: "Queued while setup finishes.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: AutonomousToolRuntime::for_project(
            &app.handle().clone(),
            app.state::<DesktopState>().inner(),
            &project_id,
        )
        .expect("build continuation tool runtime"),
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
        answer_pending_actions: false,
        auto_compact: None,
    })
    .expect("queue continuation while environment is not ready");

    assert!(!queued
        .messages
        .iter()
        .any(|message| message.content == "Queued while setup finishes."));
    let pending = db::project_store::list_undelivered_agent_environment_pending_messages(
        &repo_root,
        &project_id,
        run_id,
    )
    .expect("load pending environment messages");
    assert_eq!(pending.len(), 1);

    let driven =
        drive_owned_agent_run(request, AgentRunCancellationToken::default()).expect("drive run");
    assert_eq!(
        driven.run.status,
        db::project_store::AgentRunStatus::Completed
    );
    assert!(driven
        .messages
        .iter()
        .any(|message| message.content == "Queued while setup finishes."));
    assert!(
        db::project_store::list_undelivered_agent_environment_pending_messages(
            &repo_root,
            &project_id,
            run_id,
        )
        .expect("reload pending environment messages")
        .is_empty()
    );
    let ready_event = driven
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::EnvironmentLifecycleUpdate
                && serde_json::from_str::<serde_json::Value>(&event.payload_json)
                    .ok()
                    .and_then(|payload| payload["state"].as_str().map(ToOwned::to_owned))
                    .as_deref()
                    == Some("ready")
        })
        .expect("ready lifecycle event");
    let queued_message_event = driven
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::MessageDelta
                && event.payload_json.contains("Queued while setup finishes.")
        })
        .expect("queued message delivery event");
    assert!(ready_event.id < queued_message_event.id);
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
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
fn provider_history_replay_preserves_tool_call_ids() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let run_id = "owned-run-provider-history-replay-1";
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
        prompt: "Inspect before resume.\ntool:read src/tracked.txt".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("initial owned-agent run with tool call");

    let tool_message = initial
        .messages
        .iter()
        .find(|message| message.role == db::project_store::AgentMessageRole::Tool)
        .expect("tool result message should be persisted");
    let tool_payload =
        serde_json::from_str::<serde_json::Value>(&tool_message.content).expect("tool result json");
    assert!(tool_payload
        .get("toolCallId")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|id| !id.is_empty()));
    assert!(tool_payload
        .get("toolName")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|name| !name.is_empty()));
    assert!(tool_payload
        .get("parentAssistantMessageId")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|id| !id.is_empty()));

    let continue_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build autonomous tool runtime for replay");
    let continued = continue_owned_agent_run(ContinueOwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        run_id: run_id.into(),
        prompt: "Continue from replayable provider history.".into(),
        attachments: Vec::new(),
        controls: None,
        tool_runtime: continue_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
        answer_pending_actions: false,
        auto_compact: None,
    })
    .expect("continuation should rebuild valid tool-call history");

    assert_eq!(
        continued.run.status,
        db::project_store::AgentRunStatus::Completed
    );
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
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
    let change_group_id = file_change
        .change_group_id
        .as_deref()
        .expect("file change should link code change group");
    assert_eq!(payload["codeChangeGroupId"], change_group_id);

    let connection =
        Connection::open(db::database_path_for_repo(&repo_root)).expect("open project db");
    let (change_kind, status, before_snapshot_id, after_snapshot_id, file_version_count): (
        String,
        String,
        String,
        String,
        i64,
    ) = connection
        .query_row(
            r#"
            SELECT
                code_change_groups.change_kind,
                code_change_groups.status,
                code_change_groups.before_snapshot_id,
                code_change_groups.after_snapshot_id,
                COUNT(code_file_versions.id)
            FROM code_change_groups
            LEFT JOIN code_file_versions
              ON code_file_versions.project_id = code_change_groups.project_id
             AND code_file_versions.change_group_id = code_change_groups.change_group_id
            WHERE code_change_groups.project_id = ?1
              AND code_change_groups.change_group_id = ?2
            GROUP BY code_change_groups.change_group_id
            "#,
            params![project_id.as_str(), change_group_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                ))
            },
        )
        .expect("code change group row");
    assert_eq!(change_kind, "file_tool");
    assert_eq!(status, "completed");
    assert!(before_snapshot_id.starts_with("code-snapshot-"));
    assert!(after_snapshot_id.starts_with("code-snapshot-"));
    assert_eq!(file_version_count, 1);
    let (commit_id, commit_epoch, patch_file_count, patch_operation, merge_policy, hunk_count): (
        String,
        i64,
        i64,
        String,
        String,
        i64,
    ) = connection
        .query_row(
            r#"
            SELECT
                code_commits.commit_id,
                code_commits.workspace_epoch,
                code_patchsets.file_count,
                code_patch_files.operation,
                code_patch_files.merge_policy,
                code_patch_files.text_hunk_count
            FROM code_commits
            JOIN code_patchsets
              ON code_patchsets.project_id = code_commits.project_id
             AND code_patchsets.patchset_id = code_commits.patchset_id
            JOIN code_patch_files
              ON code_patch_files.project_id = code_patchsets.project_id
             AND code_patch_files.patchset_id = code_patchsets.patchset_id
            WHERE code_commits.project_id = ?1
              AND code_commits.change_group_id = ?2
            "#,
            params![project_id.as_str(), change_group_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )
        .expect("code history commit row");
    assert!(commit_id.starts_with("code-commit-"));
    assert_eq!(commit_epoch, 1);
    assert_eq!(patch_file_count, 1);
    assert_eq!(patch_operation, "modify");
    assert_eq!(merge_policy, "text");
    assert_eq!(hunk_count, 1);
    let workspace_head = db::project_store::read_code_workspace_head(&repo_root, &project_id)
        .expect("read code workspace head")
        .expect("code workspace head");
    assert_eq!(workspace_head.head_id.as_deref(), Some(commit_id.as_str()));
    assert_eq!(workspace_head.workspace_epoch, 1);
    let path_epoch =
        db::project_store::read_code_path_epoch(&repo_root, &project_id, "src/tracked.txt")
            .expect("read path epoch")
            .expect("path epoch");
    assert_eq!(path_epoch.workspace_epoch, 1);
    assert_eq!(path_epoch.commit_id.as_deref(), Some(commit_id.as_str()));

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
fn owned_agent_command_mutation_records_broad_code_change_group() {
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let seed_tool_runtime = AutonomousToolRuntime::for_project(
        &app.handle().clone(),
        app.state::<DesktopState>().inner(),
        &project_id,
    )
    .expect("build seed autonomous tool runtime");
    let seed_snapshot = run_owned_agent_task(OwnedAgentRunRequest {
        repo_root: repo_root.clone(),
        project_id: project_id.clone(),
        agent_session_id: db::project_store::DEFAULT_AGENT_SESSION_ID.into(),
        run_id: "owned-run-explicit-generated-seed-1".into(),
        prompt: "Seed an explicit generated path.\ntool:mkdir target\ntool:write target/explicit.txt seed\n"
            .into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime: seed_tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("owned agent generated seed run succeeds");
    assert_eq!(
        seed_snapshot.run.status,
        db::project_store::AgentRunStatus::Completed,
        "last error: {:?}",
        seed_snapshot.run.last_error
    );
    fs::write(repo_root.join("delete-me.txt"), "remove me\n").expect("seed file for delete");

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
        run_id: "owned-run-command-code-change-1".into(),
        prompt: [
            "Run a command that mutates broad file state.",
            "tool:command_sh python3 -c \"from pathlib import Path; Path('src/tracked.txt').write_text('changed'); Path('binary.bin').write_bytes(bytes([0, 1]) + b'binary'); Path('delete-me.txt').unlink(); Path('target/explicit.txt').write_text('mutated')\"",
        ]
        .join("\n"),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("owned agent command run succeeds");
    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Completed,
        "last error: {:?}",
        snapshot.run.last_error
    );
    assert!(
        snapshot
            .tool_calls
            .iter()
            .any(|tool_call| tool_call.tool_name == "command_run"
                && tool_call.state == db::project_store::AgentToolCallState::Succeeded),
        "tool calls: {:?}",
        snapshot
            .tool_calls
            .iter()
            .map(|tool_call| (
                tool_call.tool_name.as_str(),
                format!("{:?}", tool_call.state),
                tool_call.error.as_ref().map(|error| error.code.as_str())
            ))
            .collect::<Vec<_>>()
    );

    let connection =
        Connection::open(db::database_path_for_repo(&repo_root)).expect("open project db");
    let (change_group_id, change_kind, status): (String, String, String) = connection
        .query_row(
            r#"
            SELECT change_group_id, change_kind, status
            FROM code_change_groups
            WHERE project_id = ?1
              AND run_id = 'owned-run-command-code-change-1'
              AND change_kind = 'command'
            "#,
            params![project_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("command code change group");
    assert_eq!(change_kind, "command");
    assert_eq!(status, "completed");
    let (commit_id, workspace_epoch, file_count): (String, i64, i64) = connection
        .query_row(
            r#"
            SELECT
                code_commits.commit_id,
                code_commits.workspace_epoch,
                code_patchsets.file_count
            FROM code_commits
            JOIN code_patchsets
              ON code_patchsets.project_id = code_commits.project_id
             AND code_patchsets.patchset_id = code_commits.patchset_id
            WHERE code_commits.project_id = ?1
              AND code_commits.change_group_id = ?2
              AND code_commits.run_id = 'owned-run-command-code-change-1'
            "#,
            params![project_id.as_str(), change_group_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .expect("command code history commit");
    assert!(commit_id.starts_with("code-commit-"));
    assert_eq!(workspace_epoch, 3);
    assert_eq!(file_count, 4);
    let patch_files = {
        let mut statement = connection
            .prepare(
                r#"
                SELECT
                    path_before,
                    path_after,
                    operation,
                    merge_policy,
                    text_hunk_count,
                    result_blob_id
                FROM code_patch_files
                JOIN code_commits
                  ON code_commits.project_id = code_patch_files.project_id
                 AND code_commits.patchset_id = code_patch_files.patchset_id
                WHERE code_commits.project_id = ?1
                  AND code_commits.change_group_id = ?2
                ORDER BY code_patch_files.path_before, code_patch_files.path_after
                "#,
            )
            .expect("prepare command patch file query");
        statement
            .query_map(
                params![project_id.as_str(), change_group_id.as_str()],
                |row| {
                    Ok((
                        row.get::<_, Option<String>>(0)?,
                        row.get::<_, Option<String>>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                        row.get::<_, Option<String>>(5)?,
                    ))
                },
            )
            .expect("query command patch files")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect command patch files")
    };
    assert!(patch_files
        .iter()
        .any(|(_, path_after, operation, merge_policy, hunk_count, _)| {
            path_after.as_deref() == Some("src/tracked.txt")
                && operation == "modify"
                && merge_policy == "text"
                && *hunk_count == 1
        }));
    assert!(patch_files.iter().any(
        |(_, path_after, operation, merge_policy, hunk_count, result_blob_id)| {
            path_after.as_deref() == Some("binary.bin")
                && operation == "create"
                && merge_policy == "exact"
                && *hunk_count == 0
                && result_blob_id
                    .as_deref()
                    .is_some_and(|hash| hash.len() == 64)
        }
    ));
    assert!(patch_files
        .iter()
        .any(|(path_before, path_after, operation, _, _, _)| {
            path_before.as_deref() == Some("delete-me.txt")
                && path_after.is_none()
                && operation == "delete"
        }));
    assert!(patch_files
        .iter()
        .any(|(_, path_after, operation, merge_policy, hunk_count, _)| {
            path_after.as_deref() == Some("target/explicit.txt")
                && operation == "modify"
                && merge_policy == "text"
                && *hunk_count == 1
        }));
    let generated_version_flags: (i64, i64) = connection
        .query_row(
            r#"
            SELECT generated, explicitly_edited
            FROM code_file_versions
            WHERE project_id = ?1
              AND change_group_id = ?2
              AND path_after = 'target/explicit.txt'
            "#,
            params![project_id.as_str(), change_group_id.as_str()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("generated explicit broad file version");
    assert_eq!(generated_version_flags, (1, 1));
    let workspace_head = db::project_store::read_code_workspace_head(&repo_root, &project_id)
        .expect("read code workspace head")
        .expect("code workspace head");
    assert_eq!(workspace_head.head_id.as_deref(), Some(commit_id.as_str()));
    assert_eq!(workspace_head.workspace_epoch, 3);
    let generated_path_epoch =
        db::project_store::read_code_path_epoch(&repo_root, &project_id, "target/explicit.txt")
            .expect("read generated path epoch")
            .expect("generated path epoch");
    assert_eq!(generated_path_epoch.workspace_epoch, 3);
    assert_eq!(
        generated_path_epoch.commit_id.as_deref(),
        Some(commit_id.as_str())
    );
    assert_eq!(
        fs::read_to_string(repo_root.join("src").join("tracked.txt")).expect("command text file"),
        "changed"
    );
    assert_eq!(
        fs::read(repo_root.join("binary.bin")).expect("command binary file"),
        vec![0, 1, b'b', b'i', b'n', b'a', b'r', b'y']
    );
    assert!(!repo_root.join("delete-me.txt").exists());
    assert_eq!(
        fs::read_to_string(repo_root.join("target").join("explicit.txt"))
            .expect("generated explicit file"),
        "mutated"
    );
}

#[test]
fn owned_agent_verification_command_write_is_recovered_mutation() {
    if std::process::Command::new("npm")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }

    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    fs::write(
        repo_root.join("package.json"),
        r#"{"scripts":{"test":"node -e \"require('fs').writeFileSync('recovered.txt','recovered')\""}} "#,
    )
    .expect("write package");
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
        run_id: "owned-run-recovered-mutation-1".into(),
        prompt: "Run focused verification.\ntool:command_verify npm test".into(),
        attachments: Vec::new(),
        controls: Some(yolo_controls_input()),
        tool_runtime,
        provider_config: AgentProviderConfig::Fake,
        provider_preflight: None,
    })
    .expect("owned agent verification run returns a snapshot");

    let file_change = snapshot
        .file_changes
        .iter()
        .find(|change| change.path == "recovered.txt")
        .expect("recovered mutation file change");
    let change_group_id = file_change
        .change_group_id
        .as_deref()
        .expect("recovered mutation should link code change group");
    let connection =
        Connection::open(db::database_path_for_repo(&repo_root)).expect("open project db");
    let change_kind: String = connection
        .query_row(
            "SELECT change_kind FROM code_change_groups WHERE project_id = ?1 AND change_group_id = ?2",
            params![project_id, change_group_id],
            |row| row.get(0),
        )
        .expect("recovered mutation code change group");
    assert_eq!(change_kind, "recovered_mutation");
    assert_eq!(
        fs::read_to_string(repo_root.join("recovered.txt")).expect("recovered file"),
        "recovered"
    );
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
        answer_pending_actions: true,
        auto_compact: None,
    })
    .expect("approved safety action should replay original tool call");

    assert_eq!(
        resumed.run.status,
        db::project_store::AgentRunStatus::Completed,
        "last error: {:?}; action requests: {:?}",
        resumed.run.last_error,
        resumed
            .action_requests
            .iter()
            .map(|action| (
                action.action_id.as_str(),
                action.action_type.as_str(),
                action.status.as_str()
            ))
            .collect::<Vec<_>>()
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
        provider_preflight: None,
    })
    .expect("owned agent run should persist stale-write safety decision");

    assert_eq!(
        snapshot.run.status,
        db::project_store::AgentRunStatus::Paused
    );
    assert!(snapshot.tool_calls.iter().any(|tool_call| {
        tool_call.tool_name == "command_run"
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        provider_preflight: None,
    })
    .expect("owned agent command task succeeds");

    let partial_output = snapshot
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::CommandOutput
                && event.payload_json.contains(r#""partial":true"#)
        })
        .expect("partial command output event");
    let partial_payload: serde_json::Value = serde_json::from_str(&partial_output.payload_json)
        .expect("partial command output payload JSON");
    assert_eq!(partial_payload["toolCallId"], "tool-call-command-1");
    assert_eq!(partial_payload["toolName"], "command_probe");
    assert_eq!(partial_payload["stream"], "stdout");
    assert_eq!(partial_payload["text"], "hello-xero");

    let command_output = snapshot
        .events
        .iter()
        .find(|event| {
            event.event_kind == db::project_store::AgentRunEventKind::CommandOutput
                && event.payload_json.contains(r#""exitCode":0"#)
        })
        .expect("final command_output event");
    let payload: serde_json::Value =
        serde_json::from_str(&command_output.payload_json).expect("command output payload JSON");
    assert_eq!(payload["toolCallId"], "tool-call-command-1");
    assert_eq!(payload["toolName"], "command_probe");
    assert_eq!(payload["argv"], json!(["echo", "hello-xero"]));
    assert_eq!(payload["stdout"], "hello-xero");
    assert_eq!(payload["spawned"], true);
    assert_eq!(payload["exitCode"], 0);
    assert_eq!(payload["sandbox"]["profile"], "full_local_with_approval");
    #[cfg(target_os = "macos")]
    assert_eq!(
        payload["sandbox"]["platformPlan"]["strategy"],
        "macos_sandbox_exec"
    );
    assert_eq!(payload["sandbox"]["exitClassification"], "success");
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
        provider_preflight: None,
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
        provider_preflight: None,
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
        .find(|tool_call| tool_call.tool_name == "command_run")
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
        tool_call.tool_name == "command_run"
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
