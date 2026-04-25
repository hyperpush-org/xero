pub(crate) use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader, Write},
    net::TcpListener,
    path::{Path, PathBuf},
    sync::{
        mpsc::{sync_channel, Receiver},
        Arc, Mutex, MutexGuard, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

pub(crate) use cadence_desktop_lib::{
    auth::{persist_openai_codex_session, OpenRouterAuthConfig, StoredOpenAiCodexSession},
    commands::{
        get_autonomous_run::get_autonomous_run, get_project_snapshot::get_project_snapshot,
        get_runtime_run::get_runtime_run, start_runtime_session::start_runtime_session,
        stop_runtime_run::stop_runtime_run, submit_notification_reply::submit_notification_reply,
        upsert_runtime_settings::upsert_runtime_settings, AutonomousRunStateDto,
        AutonomousRunStatusDto, AutonomousSkillCacheStatusDto, AutonomousSkillLifecycleResultDto,
        AutonomousSkillLifecycleStageDto, AutonomousUnitKindDto, AutonomousUnitStatusDto,
        GetAutonomousRunRequestDto, GetRuntimeRunRequestDto, NotificationDispatchStatusDto,
        NotificationReplyClaimStatusDto, OperatorApprovalStatus, PhaseStatus, PhaseStep,
        ProjectIdRequestDto, ResumeHistoryStatus, RuntimeAuthPhase, RuntimeRunCheckpointKindDto,
        RuntimeRunStatusDto, RuntimeRunTransportLivenessDto, RuntimeSessionDto,
        RuntimeStreamItemDto, RuntimeStreamItemKind, StopRuntimeRunRequestDto,
        SubmitNotificationReplyRequestDto, UpsertRuntimeSettingsRequestDto,
    },
    configure_builder_with_state,
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    runtime::{
        autonomous_orchestrator::persist_supervisor_event, launch_detached_runtime_supervisor,
        protocol::SupervisorLiveEventPayload, start_runtime_stream, AutonomousSkillRuntime,
        AutonomousSkillRuntimeConfig, AutonomousSkillSource, AutonomousSkillSourceEntryKind,
        AutonomousSkillSourceError, AutonomousSkillSourceFileRequest,
        AutonomousSkillSourceFileResponse, AutonomousSkillSourceTreeEntry,
        AutonomousSkillSourceTreeRequest, AutonomousSkillSourceTreeResponse,
        FilesystemAutonomousSkillCacheStore, RuntimeStreamRequest, RuntimeSupervisorLaunchRequest,
    },
    state::DesktopState,
};
pub(crate) use serde_json::json;
pub(crate) use tauri::Manager;
pub(crate) use tempfile::TempDir;

#[path = "../support/runtime_shell.rs"]
pub(crate) mod runtime_shell;

#[path = "../support/supervisor_test_lock.rs"]
pub(crate) mod supervisor_test_lock;

pub(crate) const STRUCTURED_EVENT_PREFIX: &str = "__Cadence_EVENT__ ";

pub(crate) struct SupervisorTestGuard {
    _in_process: MutexGuard<'static, ()>,
    _cross_process: supervisor_test_lock::SupervisorProcessLock,
}

impl Drop for SupervisorTestGuard {
    fn drop(&mut self) {
        // Detached supervisor teardown can lag slightly behind the point where the test body has
        // already observed a stopped snapshot. Keep the guard held for one short cool-down so the
        // next parity test does not begin while the prior sidecar is still unwinding.
        thread::sleep(Duration::from_millis(500));
    }
}

pub(crate) fn supervisor_test_guard() -> SupervisorTestGuard {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    let in_process = GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    SupervisorTestGuard {
        _in_process: in_process,
        _cross_process: supervisor_test_lock::lock_supervisor_test_process(),
    }
}

pub(crate) fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

pub(crate) fn create_state(root: &TempDir) -> DesktopState {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");
    let provider_profiles_path = root.path().join("app-data").join("provider-profiles.json");
    let provider_profile_credentials_path = root
        .path()
        .join("app-data")
        .join("provider-profile-credentials.json");
    let runtime_settings_path = root.path().join("app-data").join("runtime-settings.json");
    let openrouter_credential_path = root
        .path()
        .join("app-data")
        .join("openrouter-credentials.json");

    DesktopState::default()
        .with_registry_file_override(registry_path)
        .with_auth_store_file_override(auth_store_path)
        .with_provider_profiles_file_override(provider_profiles_path)
        .with_provider_profile_credential_store_file_override(provider_profile_credentials_path)
        .with_runtime_settings_file_override(runtime_settings_path)
        .with_openrouter_credential_file_override(openrouter_credential_path)
        .with_autonomous_skill_cache_dir_override(
            root.path().join("app-data").join("autonomous-skills"),
        )
        .with_runtime_supervisor_binary_override(supervisor_binary_path())
}

pub(crate) fn create_openrouter_state(root: &TempDir, models_url: String) -> DesktopState {
    create_state(root).with_openrouter_auth_config_override(openrouter_auth_config(models_url))
}

pub(crate) fn openrouter_auth_config(models_url: String) -> OpenRouterAuthConfig {
    OpenRouterAuthConfig {
        models_url,
        timeout: Duration::from_secs(5),
    }
}

pub(crate) fn spawn_static_http_server(status: u16, body: &str) -> String {
    spawn_static_http_server_with_requests(status, body, 1)
}

pub(crate) fn spawn_static_http_server_with_requests(
    status: u16,
    body: &str,
    request_count: usize,
) -> String {
    assert!(
        request_count > 0,
        "test http server must handle at least one request"
    );

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test http server");
    let address = listener.local_addr().expect("test http server addr");
    let body = body.to_owned();

    thread::spawn(move || {
        for _ in 0..request_count {
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
                "HTTP/1.1 {status} Test\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            )
            .expect("write test http response");
        }
    });

    format!("http://{address}")
}

pub(crate) fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_Cadence-runtime-supervisor"))
}

pub(crate) fn seed_project(
    root: &TempDir,
    app: &tauri::App<tauri::test::MockRuntime>,
) -> (String, PathBuf) {
    let repo_root = root.path().join("repo");
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();

    let repository = CanonicalRepository {
        project_id: "project-1".into(),
        repository_id: "repo-1".into(),
        root_path: canonical_root.clone(),
        root_path_string: root_path_string.clone(),
        common_git_dir: canonical_root.join(".git"),
        display_name: "repo".into(),
        branch_name: Some("main".into()),
        head_sha: Some("abc123".into()),
        branch: None,
        last_commit: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

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

pub(crate) fn seed_planning_lifecycle_workflow(repo_root: &Path, project_id: &str) {
    project_store::upsert_workflow_graph(
        repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "discussion".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Discussion".into(),
                    description: "Clarify project intent.".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Discuss),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "research".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Research".into(),
                    description: "Gather constraints.".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "requirements".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Requirements".into(),
                    description: "Lock requirement deltas.".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "roadmap".into(),
                    phase_id: 4,
                    sort_order: 4,
                    name: "Roadmap".into(),
                    description: "Plan downstream slices.".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Verify),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "discussion".into(),
                    to_node_id: "research".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "research".into(),
                    to_node_id: "requirements".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "requirements".into(),
                    to_node_id: "roadmap".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
            ],
            gates: vec![],
        },
    )
    .expect("seed planning lifecycle workflow");
}

pub(crate) fn upsert_notification_route(
    repo_root: &Path,
    project_id: &str,
    route_id: &str,
    route_kind: &str,
    route_target: &str,
) {
    project_store::upsert_notification_route(
        repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.into(),
            route_id: route_id.into(),
            route_kind: route_kind.into(),
            route_target: route_target.into(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-18T18:59:58Z".into(),
        },
    )
    .expect("upsert notification route");
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn launch_scripted_runtime_run_with_runtime_kind(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    runtime_kind: &str,
    run_id: &str,
    session_id: &str,
    flow_id: Option<&str>,
    script: &str,
) -> project_store::RuntimeRunSnapshotRecord {
    let shell = runtime_shell::launch_script(script);

    launch_detached_runtime_supervisor(
        state,
        RuntimeSupervisorLaunchRequest {
            project_id: project_id.into(),
            agent_session_id: "agent-session-main".into(),
            repo_root: repo_root.to_path_buf(),
            runtime_kind: runtime_kind.into(),
            run_id: run_id.into(),
            session_id: session_id.into(),
            flow_id: flow_id.map(str::to_string),
            launch_context: cadence_desktop_lib::runtime::RuntimeSupervisorLaunchContext {
                provider_id: runtime_kind.into(),
                session_id: session_id.into(),
                flow_id: flow_id.map(str::to_string),
                model_id: "openai_codex".into(),
                thinking_effort: None,
            },
            launch_env: cadence_desktop_lib::runtime::RuntimeSupervisorLaunchEnv::default(),
            program: shell.program,
            args: shell.args,
            startup_timeout: Duration::from_secs(5),
            control_timeout: Duration::from_millis(750),
            supervisor_binary: state.runtime_supervisor_binary_override().cloned(),
            run_controls: RuntimeSupervisorLaunchRequest::default().run_controls,
        },
    )
    .expect("launch scripted runtime supervisor")
}

pub(crate) fn launch_scripted_runtime_run(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    session_id: &str,
    flow_id: Option<&str>,
    script: &str,
) -> project_store::RuntimeRunSnapshotRecord {
    launch_scripted_runtime_run_with_runtime_kind(
        state,
        repo_root,
        project_id,
        "openai_codex",
        run_id,
        session_id,
        flow_id,
        script,
    )
}

pub(crate) fn wait_for_runtime_run(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    predicate: impl Fn(&cadence_desktop_lib::commands::RuntimeRunDto) -> bool,
) -> cadence_desktop_lib::commands::RuntimeRunDto {
    let deadline = Instant::now() + Duration::from_secs(10);

    loop {
        let runtime_run = get_runtime_run(
            app.handle().clone(),
            app.state::<DesktopState>(),
            GetRuntimeRunRequestDto {
                project_id: project_id.into(),
                agent_session_id: "agent-session-main".into(),
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

pub(crate) fn wait_for_autonomous_run(
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
                agent_session_id: "agent-session-main".into(),
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

pub(crate) fn wait_for_notification_dispatches_for_action(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    expected_count: usize,
) -> Vec<project_store::NotificationDispatchRecord> {
    let deadline = Instant::now() + Duration::from_secs(5);

    loop {
        let dispatches =
            project_store::load_notification_dispatches(repo_root, project_id, Some(action_id))
                .expect("load notification dispatches for action");
        if dispatches.len() == expected_count {
            return dispatches;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected_count} notification dispatch row(s) for action `{action_id}`, got {dispatches:?}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

pub(crate) fn count_rows(repo_root: &Path, query: &str, params: &[&dyn rusqlite::ToSql]) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row(query, params, |row| row.get(0))
        .expect("count rows")
}

pub(crate) fn count_autonomous_unit_rows(repo_root: &Path, project_id: &str, run_id: &str) -> i64 {
    count_rows(
        repo_root,
        "SELECT COUNT(*) FROM autonomous_units WHERE project_id = ?1 AND run_id = ?2",
        &[&project_id, &run_id],
    )
}

pub(crate) fn count_autonomous_attempt_rows(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> i64 {
    count_rows(
        repo_root,
        "SELECT COUNT(*) FROM autonomous_unit_attempts WHERE project_id = ?1 AND run_id = ?2",
        &[&project_id, &run_id],
    )
}

pub(crate) fn count_workflow_transition_rows(repo_root: &Path, project_id: &str) -> i64 {
    count_rows(
        repo_root,
        "SELECT COUNT(*) FROM workflow_transition_events WHERE project_id = ?1",
        &[&project_id],
    )
}

pub(crate) fn count_workflow_handoff_rows(repo_root: &Path, project_id: &str) -> i64 {
    count_rows(
        repo_root,
        "SELECT COUNT(*) FROM workflow_handoff_packages WHERE project_id = ?1",
        &[&project_id],
    )
}

type HistoryShapeEntry = (u32, String, u32, String, String, String, String, String);

pub(crate) fn history_shape(state: &AutonomousRunStateDto) -> Vec<HistoryShapeEntry> {
    state
        .history
        .iter()
        .map(|entry| {
            let attempt = entry
                .latest_attempt
                .as_ref()
                .expect("each history entry should expose its latest attempt");
            let linkage = entry
                .unit
                .workflow_linkage
                .as_ref()
                .expect("each history entry should expose workflow linkage");
            (
                entry.unit.sequence,
                entry.unit.unit_id.clone(),
                attempt.attempt_number,
                attempt.attempt_id.clone(),
                attempt.child_session_id.clone(),
                linkage.workflow_node_id.clone(),
                linkage.transition_id.clone(),
                linkage.handoff_package_hash.clone(),
            )
        })
        .collect()
}

#[allow(dead_code)]
pub(crate) fn fixture_story_script() -> String {
    runtime_shell::script_join_steps(&[
        runtime_shell::script_sleep(2),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-inspect-1",
                "tool_name": "inspect_repository",
                "tool_state": "running",
                "detail": "Collecting deterministic fixture proof context"
            })
        )),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-inspect-1",
                "tool_name": "inspect_repository",
                "tool_state": "succeeded",
                "detail": "Collected deterministic fixture proof context"
            })
        )),
        runtime_shell::script_print_line(&format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "activity",
                "code": "policy_denied_write_access",
                "title": "Policy denied write access",
                "detail": "Cadence blocked repository writes until operator approval resumes the active boundary"
            })
        )),
        runtime_shell::script_prompt_read_echo_and_sleep(
            "Enter approval code: ",
            "value",
            "value=",
            5,
        ),
    ])
}

pub(crate) fn combined_fixture_story_script(skill_lines: &[String]) -> String {
    let mut steps = Vec::with_capacity(skill_lines.len() + 5);
    steps.push(runtime_shell::script_sleep(2));
    steps.push(runtime_shell::script_print_line(&format!(
        "{STRUCTURED_EVENT_PREFIX}{}",
        json!({
            "kind": "tool",
            "tool_call_id": "tool-inspect-1",
            "tool_name": "inspect_repository",
            "tool_state": "running",
            "detail": "Collecting deterministic fixture proof context"
        })
    )));
    steps.push(runtime_shell::script_print_line(&format!(
        "{STRUCTURED_EVENT_PREFIX}{}",
        json!({
            "kind": "tool",
            "tool_call_id": "tool-inspect-1",
            "tool_name": "inspect_repository",
            "tool_state": "succeeded",
            "detail": "Collected deterministic fixture proof context"
        })
    )));
    steps.extend(
        skill_lines
            .iter()
            .map(|line| runtime_shell::script_print_line(line)),
    );
    steps.push(runtime_shell::script_print_line(&format!(
        "{STRUCTURED_EVENT_PREFIX}{}",
        json!({
            "kind": "activity",
            "code": "policy_denied_write_access",
            "title": "Policy denied write access",
            "detail": "Cadence blocked repository writes until operator approval resumes the active boundary"
        })
    )));
    steps.push(runtime_shell::script_prompt_read_echo_and_sleep(
        "Enter approval code: ",
        "value",
        "value=",
        2,
    ));
    runtime_shell::script_join_steps(&steps)
}

pub(crate) fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

#[derive(Clone, Default)]
pub(crate) struct FixtureSkillSource {
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
    pub(crate) fn set_tree_response(
        &self,
        response: Result<AutonomousSkillSourceTreeResponse, AutonomousSkillSourceError>,
    ) {
        self.state
            .lock()
            .expect("fixture source lock")
            .tree_response = Some(response);
    }

    pub(crate) fn set_file_text(&self, repo: &str, reference: &str, path: &str, content: &str) {
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

    pub(crate) fn tree_request_count(&self) -> usize {
        self.state
            .lock()
            .expect("fixture source lock")
            .tree_requests
            .len()
    }

    pub(crate) fn file_request_count(&self) -> usize {
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

pub(crate) fn skill_runtime_config() -> AutonomousSkillRuntimeConfig {
    AutonomousSkillRuntimeConfig {
        default_source_repo: "vercel-labs/skills".into(),
        default_source_ref: "main".into(),
        default_source_root: "skills".into(),
        github_api_base_url: "https://api.github.com".into(),
        github_token: None,
        limits: Default::default(),
    }
}

pub(crate) fn standard_skill_tree(
    skill_id: &str,
    tree_hash: &str,
) -> AutonomousSkillSourceTreeResponse {
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

pub(crate) fn seed_authenticated_runtime(
    app: &tauri::App<tauri::test::MockRuntime>,
    root: &TempDir,
    project_id: &str,
) -> RuntimeSessionDto {
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");
    persist_openai_codex_session(
        &auth_store_path,
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
    runtime
}

pub(crate) fn seed_openrouter_runtime(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    secret: &str,
) -> RuntimeSessionDto {
    upsert_runtime_settings(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertRuntimeSettingsRequestDto {
            provider_id: "openrouter".into(),
            model_id: "openai/gpt-4o-mini".into(),
            openrouter_api_key: Some(secret.into()),
            anthropic_api_key: None,
        },
    )
    .expect("save openrouter runtime settings for deterministic parity proof");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("start openrouter runtime session");
    assert_eq!(runtime.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(runtime.provider_id, "openrouter");
    assert_eq!(runtime.runtime_kind, "openrouter");
    runtime
}

pub(crate) fn capture_stream_channel() -> (
    tauri::ipc::Channel<RuntimeStreamItemDto>,
    Receiver<RuntimeStreamItemDto>,
) {
    let (tx, rx) = sync_channel(32);
    let channel = tauri::ipc::Channel::<RuntimeStreamItemDto>::new(move |body| {
        tx.send(
            body.deserialize::<RuntimeStreamItemDto>()
                .expect("deserialize runtime stream item"),
        )
        .expect("send runtime stream item to test receiver");
        Ok(())
    });

    (channel, rx)
}

pub(crate) fn start_direct_runtime_stream(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    repo_root: &Path,
    runtime: &RuntimeSessionDto,
    run_id: &str,
    requested_item_kinds: Vec<RuntimeStreamItemKind>,
    channel: tauri::ipc::Channel<RuntimeStreamItemDto>,
) {
    start_runtime_stream(
        app.handle().clone(),
        app.state::<DesktopState>().inner().clone(),
        RuntimeStreamRequest {
            project_id: project_id.into(),
            agent_session_id: "agent-session-main".into(),
            repo_root: repo_root.to_path_buf(),
            session_id: runtime
                .session_id
                .clone()
                .expect("authenticated runtime should have a session id"),
            flow_id: runtime.flow_id.clone(),
            runtime_kind: runtime.runtime_kind.clone(),
            run_id: run_id.into(),
            requested_item_kinds,
        },
        channel,
    );
}

pub(crate) fn collect_until_terminal(
    receiver: Receiver<RuntimeStreamItemDto>,
) -> Vec<RuntimeStreamItemDto> {
    let mut items = Vec::new();

    loop {
        match receiver.recv_timeout(Duration::from_secs(5)) {
            Ok(item) => {
                let terminal = matches!(
                    item.kind,
                    RuntimeStreamItemKind::Complete | RuntimeStreamItemKind::Failure
                );
                items.push(item);
                if terminal {
                    return items;
                }
            }
            Err(error) => panic!("timed out waiting for runtime stream items: {error}"),
        }
    }
}

pub(crate) fn assert_monotonic_sequences(items: &[RuntimeStreamItemDto], expected_run_id: &str) {
    let mut previous = None;
    for item in items {
        assert_eq!(item.run_id, expected_run_id);
        if let Some(previous) = previous {
            assert!(
                item.sequence > previous,
                "expected strictly increasing sequences, got {previous} then {} in {items:?}",
                item.sequence
            );
        }
        previous = Some(item.sequence);
    }
}

pub(crate) fn load_skill_payload_jsons(repo_root: &Path) -> Vec<String> {
    let connection = rusqlite::Connection::open(database_path_for_repo(repo_root))
        .expect("open runtime db for skill payloads");
    let mut statement = connection
        .prepare(
            "SELECT payload_json FROM autonomous_unit_artifacts WHERE artifact_kind = 'skill_lifecycle' ORDER BY artifact_id",
        )
        .expect("prepare skill payload query");
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .expect("query skill payload rows");

    rows.map(|row| row.expect("decode skill payload row"))
        .collect()
}
