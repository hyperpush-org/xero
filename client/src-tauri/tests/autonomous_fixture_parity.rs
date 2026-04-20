use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        mpsc::{sync_channel, Receiver},
        Arc, Mutex, MutexGuard, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use cadence_desktop_lib::{
    auth::{persist_openai_codex_session, StoredOpenAiCodexSession},
    commands::{
        get_autonomous_run::get_autonomous_run, get_project_snapshot::get_project_snapshot,
        get_runtime_run::get_runtime_run, start_runtime_session::start_runtime_session,
        stop_runtime_run::stop_runtime_run, submit_notification_reply::submit_notification_reply,
        AutonomousRunStateDto, AutonomousRunStatusDto, AutonomousSkillCacheStatusDto,
        AutonomousSkillLifecycleResultDto, AutonomousSkillLifecycleStageDto,
        AutonomousUnitKindDto, AutonomousUnitStatusDto, GetAutonomousRunRequestDto,
        GetRuntimeRunRequestDto, NotificationDispatchStatusDto,
        NotificationReplyClaimStatusDto, OperatorApprovalStatus, PhaseStatus, PhaseStep,
        ProjectIdRequestDto, ResumeHistoryStatus, RuntimeAuthPhase,
        RuntimeRunCheckpointKindDto, RuntimeRunStatusDto, RuntimeRunTransportLivenessDto,
        RuntimeSessionDto, RuntimeStreamItemDto, RuntimeStreamItemKind,
        StopRuntimeRunRequestDto, SubmitNotificationReplyRequestDto,
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
        AutonomousSkillSourceFileResponse, AutonomousSkillSourceMetadata,
        AutonomousSkillSourceTreeEntry, AutonomousSkillSourceTreeRequest,
        AutonomousSkillSourceTreeResponse, FilesystemAutonomousSkillCacheStore,
        RuntimeStreamRequest, RuntimeSupervisorLaunchRequest,
    },
    state::DesktopState,
};
use serde_json::json;
use tauri::Manager;
use tempfile::TempDir;

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

const STRUCTURED_EVENT_PREFIX: &str = "__CADENCE_EVENT__ ";

fn supervisor_test_guard() -> MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> DesktopState {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");

    DesktopState::default()
        .with_registry_file_override(registry_path)
        .with_auth_store_file_override(auth_store_path)
        .with_autonomous_skill_cache_dir_override(
            root.path().join("app-data").join("autonomous-skills"),
        )
        .with_runtime_supervisor_binary_override(supervisor_binary_path())
}

fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cadence-runtime-supervisor"))
}

fn seed_project(root: &TempDir, app: &tauri::App<tauri::test::MockRuntime>) -> (String, PathBuf) {
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

fn seed_planning_lifecycle_workflow(repo_root: &Path, project_id: &str) {
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

fn upsert_notification_route(
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

fn launch_scripted_runtime_run(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
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
            repo_root: repo_root.to_path_buf(),
            runtime_kind: "openai_codex".into(),
            run_id: run_id.into(),
            session_id: session_id.into(),
            flow_id: flow_id.map(str::to_string),
            program: shell.program,
            args: shell.args,
            startup_timeout: Duration::from_secs(5),
            control_timeout: Duration::from_millis(750),
            supervisor_binary: state.runtime_supervisor_binary_override().cloned(),
        },
    )
    .expect("launch scripted runtime supervisor")
}

fn wait_for_runtime_run(
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

fn wait_for_notification_dispatches_for_action(
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

fn count_rows(repo_root: &Path, query: &str, params: &[&dyn rusqlite::ToSql]) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row(query, params, |row| row.get(0))
        .expect("count rows")
}

fn count_autonomous_unit_rows(repo_root: &Path, project_id: &str, run_id: &str) -> i64 {
    count_rows(
        repo_root,
        "SELECT COUNT(*) FROM autonomous_units WHERE project_id = ?1 AND run_id = ?2",
        &[&project_id, &run_id],
    )
}

fn count_autonomous_attempt_rows(repo_root: &Path, project_id: &str, run_id: &str) -> i64 {
    count_rows(
        repo_root,
        "SELECT COUNT(*) FROM autonomous_unit_attempts WHERE project_id = ?1 AND run_id = ?2",
        &[&project_id, &run_id],
    )
}

fn count_workflow_transition_rows(repo_root: &Path, project_id: &str) -> i64 {
    count_rows(
        repo_root,
        "SELECT COUNT(*) FROM workflow_transition_events WHERE project_id = ?1",
        &[&project_id],
    )
}

fn count_workflow_handoff_rows(repo_root: &Path, project_id: &str) -> i64 {
    count_rows(
        repo_root,
        "SELECT COUNT(*) FROM workflow_handoff_packages WHERE project_id = ?1",
        &[&project_id],
    )
}

fn history_shape(
    state: &AutonomousRunStateDto,
) -> Vec<(u32, String, u32, String, String, String, String, String)> {
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

fn fixture_story_script() -> String {
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

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
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

fn skill_runtime_config() -> AutonomousSkillRuntimeConfig {
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

fn seed_authenticated_runtime(
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

fn capture_stream_channel() -> (
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

fn start_direct_runtime_stream(
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

fn collect_until_terminal(receiver: Receiver<RuntimeStreamItemDto>) -> Vec<RuntimeStreamItemDto> {
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

fn assert_monotonic_sequences(items: &[RuntimeStreamItemDto], expected_run_id: &str) {
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

fn load_skill_payload_jsons(repo_root: &Path) -> Vec<String> {
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

#[test]
fn autonomous_fixture_repo_parity_proves_stage_rollover_boundary_resume_and_reload_identity() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_planning_lifecycle_workflow(&repo_root, &project_id);
    upsert_notification_route(
        &repo_root,
        &project_id,
        "route-telegram",
        "telegram",
        "telegram:ops-room",
    );
    upsert_notification_route(
        &repo_root,
        &project_id,
        "route-discord",
        "discord",
        "discord:ops-room",
    );

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-autonomous-fixture-parity",
        "session-1",
        Some("flow-1"),
        &fixture_story_script(),
    );

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });

    let progressed = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        let Some(unit) = autonomous_state.unit.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous_state.attempt.as_ref() else {
            return false;
        };
        let Some(linkage) = unit.workflow_linkage.as_ref() else {
            return false;
        };

        run.run_id == launched.run.run_id
            && autonomous_state.history.len() == 3
            && unit.sequence == 3
            && attempt.attempt_number == 3
            && unit.kind == AutonomousUnitKindDto::Planner
            && linkage.workflow_node_id == "roadmap"
            && attempt.workflow_linkage.as_ref() == Some(linkage)
    });

    let progressed_run = progressed
        .run
        .as_ref()
        .expect("progressed autonomous run should exist");
    let progressed_unit = progressed
        .unit
        .as_ref()
        .expect("progressed autonomous unit should exist");
    let progressed_attempt = progressed
        .attempt
        .as_ref()
        .expect("progressed autonomous attempt should exist");
    let progressed_linkage = progressed_unit
        .workflow_linkage
        .as_ref()
        .expect("progressed unit should expose workflow linkage");
    let progressed_shape = history_shape(&progressed);

    assert_eq!(progressed_run.run_id, launched.run.run_id);
    assert_eq!(progressed_unit.run_id, launched.run.run_id);
    assert_eq!(progressed_attempt.run_id, launched.run.run_id);
    assert_eq!(
        progressed_run.active_unit_id.as_deref(),
        Some(progressed_unit.unit_id.as_str())
    );
    assert_eq!(
        progressed_run.active_attempt_id.as_deref(),
        Some(progressed_attempt.attempt_id.as_str())
    );
    assert_eq!(
        count_autonomous_unit_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(
        count_autonomous_attempt_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 3);
    assert_eq!(
        progressed
            .history
            .iter()
            .map(|entry| entry.unit.sequence)
            .collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert_eq!(
        progressed
            .history
            .iter()
            .map(|entry| {
                entry
                    .unit
                    .workflow_linkage
                    .as_ref()
                    .expect("workflow linkage")
                    .workflow_node_id
                    .clone()
            })
            .collect::<Vec<_>>(),
        vec!["roadmap", "requirements", "research"]
    );

    let durable_progressed = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable progressed autonomous run")
        .expect("durable progressed autonomous run should exist");
    let roadmap_transition =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load lifecycle transition events")
            .into_iter()
            .find(|event| event.to_node_id == "roadmap")
            .expect("roadmap transition should exist");
    let roadmap_handoff = project_store::load_workflow_handoff_package(
        &repo_root,
        &project_id,
        &roadmap_transition.transition_id,
    )
    .expect("load roadmap handoff package")
    .expect("roadmap handoff package should exist");
    assert_eq!(progressed_linkage.workflow_node_id, "roadmap");
    assert_eq!(
        progressed_linkage.transition_id,
        roadmap_transition.transition_id
    );
    assert_eq!(
        progressed_linkage.causal_transition_id.as_deref(),
        roadmap_transition.causal_transition_id.as_deref()
    );
    assert_eq!(
        progressed_linkage.handoff_transition_id,
        roadmap_handoff.handoff_transition_id
    );
    assert_eq!(
        progressed_linkage.handoff_package_hash,
        roadmap_handoff.package_hash
    );
    assert_eq!(durable_progressed.history.len(), 3);

    thread::sleep(Duration::from_secs(3));
    let boundary_id = "boundary-1".to_string();
    let persisted_boundary = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: launched.run.run_id.clone(),
            runtime_kind: launched.run.runtime_kind.clone(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: launched.run.transport.endpoint.clone(),
            started_at: launched.run.started_at.clone(),
            last_heartbeat_at: launched.run.last_heartbeat_at.clone(),
            last_error: None,
            boundary_id: boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-18T19:00:00Z".into(),
        },
    )
    .expect("persist runtime action-required boundary for fixture parity proof");
    let action_id = persisted_boundary.approval_request.action_id.clone();
    persist_supervisor_event(
        &repo_root,
        &project_id,
        &SupervisorLiveEventPayload::ActionRequired {
            action_id: action_id.clone(),
            boundary_id: boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
        },
    )
    .expect("persist autonomous action-required event for fixture parity proof")
    .expect("autonomous action-required persistence should return a snapshot");

    let paused = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        let Some(unit) = autonomous_state.unit.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous_state.attempt.as_ref() else {
            return false;
        };

        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Paused
            && unit.status == AutonomousUnitStatusDto::Blocked
            && attempt.status == AutonomousUnitStatusDto::Blocked
            && unit.boundary_id == attempt.boundary_id
            && autonomous_state.history.len() == 3
            && autonomous_state
                .history
                .first()
                .is_some_and(|entry| entry.artifacts.len() >= 4)
    });

    let paused_run = paused
        .run
        .as_ref()
        .expect("paused autonomous run should exist");
    let paused_unit = paused
        .unit
        .as_ref()
        .expect("paused autonomous unit should exist");
    let paused_attempt = paused
        .attempt
        .as_ref()
        .expect("paused autonomous attempt should exist");
    let paused_shape = history_shape(&paused);

    assert_eq!(paused_shape, progressed_shape);
    assert_eq!(paused_run.run_id, launched.run.run_id);
    assert_eq!(paused_unit.sequence, 3);
    assert_eq!(paused_attempt.attempt_number, 3);
    assert_eq!(
        paused_unit.workflow_linkage.as_ref(),
        Some(progressed_linkage)
    );
    assert_eq!(
        paused_attempt.workflow_linkage.as_ref(),
        Some(progressed_linkage)
    );
    assert_eq!(
        count_autonomous_unit_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(
        count_autonomous_attempt_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );

    let pending_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after autonomous boundary pause");
    assert_eq!(pending_snapshot.approval_requests.len(), 1);
    assert!(pending_snapshot.resume_history.is_empty());
    let approval = &pending_snapshot.approval_requests[0];
    assert_eq!(approval.action_id, action_id);
    assert_eq!(approval.status, OperatorApprovalStatus::Pending);
    assert!(action_id.contains(boundary_id.as_str()));

    let dispatches =
        wait_for_notification_dispatches_for_action(&repo_root, &project_id, &action_id, 2);
    let telegram_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-telegram")
        .expect("telegram dispatch row should exist");
    let discord_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-discord")
        .expect("discord dispatch row should exist");
    assert!(dispatches
        .iter()
        .all(|dispatch| { dispatch.status == project_store::NotificationDispatchStatus::Pending }));

    let paused_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after boundary pause")
        .expect("durable autonomous run should exist after boundary pause");
    let paused_artifacts = paused_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.attempt_id == paused_attempt.attempt_id)
        .collect::<Vec<_>>();
    assert_eq!(paused_artifacts.len(), 4);
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "tool_result")
            .count(),
        2
    );
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "policy_denied")
            .count(),
        1
    );
    assert_eq!(
        paused_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "verification_evidence")
            .count(),
        1
    );
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-inspect-1"
                    && payload.tool_name == "inspect_repository"
                    && payload.tool_state == project_store::AutonomousToolCallStateRecord::Running
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::ToolResult(payload))
                if payload.tool_call_id == "tool-inspect-1"
                    && payload.tool_name == "inspect_repository"
                    && payload.tool_state == project_store::AutonomousToolCallStateRecord::Succeeded
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::PolicyDenied(payload))
                if payload.diagnostic_code == "policy_denied_write_access"
                    && payload.message == "Cadence blocked repository writes until operator approval resumes the active boundary"
        )
    }));
    assert!(paused_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                if payload.action_id.as_deref() == Some(action_id.as_str())
                    && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                    && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
                    && payload.evidence_kind == "terminal_input_required"
        )
    }));

    let fresh_paused_app = build_mock_app(create_state(&root));
    let recovered_paused = wait_for_autonomous_run(&fresh_paused_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Paused
            && attempt.boundary_id.as_deref() == Some(boundary_id.as_str())
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&recovered_paused), paused_shape);

    let reloaded_pending_snapshot = get_project_snapshot(
        fresh_paused_app.handle().clone(),
        fresh_paused_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load paused project snapshot after fresh host reload");
    assert_eq!(reloaded_pending_snapshot.approval_requests.len(), 1);
    assert_eq!(
        reloaded_pending_snapshot.approval_requests[0].action_id,
        action_id
    );
    assert!(reloaded_pending_snapshot.resume_history.is_empty());

    let blank_action_error = submit_notification_reply(
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: "   ".into(),
            route_id: telegram_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("telegram-operator".into()),
            reply_text: "approved".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:05Z".into(),
        },
    )
    .expect_err("blank action id should fail closed");
    assert_eq!(blank_action_error.code, "invalid_request");

    let forged_error = submit_notification_reply(
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: discord_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator".into()),
            reply_text: "forged correlation".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:06Z".into(),
        },
    )
    .expect_err("forged correlation should fail closed");
    assert_eq!(forged_error.code, "notification_reply_correlation_invalid");
    let malformed_claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load notification reply claims after malformed replies");
    assert_eq!(malformed_claims.len(), 1);
    assert!(malformed_claims.iter().all(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_correlation_invalid")
    }));
    let still_paused_snapshot = get_project_snapshot(
        fresh_paused_app.handle().clone(),
        fresh_paused_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after malformed replies");
    assert!(still_paused_snapshot.resume_history.is_empty());
    assert!(still_paused_snapshot
        .approval_requests
        .iter()
        .any(|approval_request| {
            approval_request.action_id == action_id
                && approval_request.status == OperatorApprovalStatus::Pending
        }));

    let first = submit_notification_reply(
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: telegram_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("telegram-operator".into()),
            reply_text: "approved".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:07Z".into(),
        },
    )
    .expect("first remote reply should claim, resolve, and resume exactly once");
    assert_eq!(
        first.claim.status,
        NotificationReplyClaimStatusDto::Accepted
    );
    assert_eq!(
        first.dispatch.status,
        NotificationDispatchStatusDto::Claimed
    );
    assert_eq!(
        first.resolve_result.approval_request.status,
        OperatorApprovalStatus::Approved
    );
    assert_eq!(
        first
            .resume_result
            .as_ref()
            .map(|resume| resume.resume_entry.status.clone()),
        Some(ResumeHistoryStatus::Started)
    );

    let duplicate = submit_notification_reply(
        fresh_paused_app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            route_id: discord_dispatch.route_id.clone(),
            correlation_key: discord_dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator".into()),
            reply_text: "duplicate after resume".into(),
            decision: "approve".into(),
            received_at: "2026-04-18T19:00:08Z".into(),
        },
    )
    .expect_err("duplicate reply after the winner should fail closed");
    assert_eq!(duplicate.code, "notification_reply_already_claimed");

    let resumed = wait_for_autonomous_run(&fresh_paused_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && run.status == AutonomousRunStatusDto::Running
            && attempt.boundary_id.is_none()
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&resumed), paused_shape);

    let resumed_runtime = wait_for_runtime_run(&fresh_paused_app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });
    assert_eq!(resumed_runtime.run_id, launched.run.run_id);

    let claims =
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("load reply claims after accepted and duplicate replies");
    assert_eq!(claims.len(), 3);
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Accepted
            && claim.route_id == telegram_dispatch.route_id
    }));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.route_id == discord_dispatch.route_id
            && claim.rejection_code.as_deref() == Some("notification_reply_correlation_invalid")
    }));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.route_id == discord_dispatch.route_id
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let dispatches_after =
        project_store::load_notification_dispatches(&repo_root, &project_id, Some(&action_id))
            .expect("load notification dispatches after remote replies");
    assert_eq!(
        dispatches_after
            .iter()
            .filter(|dispatch| dispatch.status == project_store::NotificationDispatchStatus::Claimed)
            .count(),
        1
    );
    assert!(dispatches_after.iter().any(|dispatch| {
        dispatch.route_id == telegram_dispatch.route_id
            && dispatch.status == project_store::NotificationDispatchStatus::Claimed
    }));

    let resumed_durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after resume")
        .expect("durable autonomous run should still exist after resume");
    let resumed_artifacts = resumed_durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.attempt_id == paused_attempt.attempt_id)
        .collect::<Vec<_>>();
    assert_eq!(resumed_artifacts.len(), 5);
    assert_eq!(
        resumed_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "tool_result")
            .count(),
        2
    );
    assert_eq!(
        resumed_artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "policy_denied")
            .count(),
        1
    );
    let boundary_evidence = resumed_artifacts
        .iter()
        .filter(|artifact| artifact.artifact_kind == "verification_evidence")
        .collect::<Vec<_>>();
    assert_eq!(boundary_evidence.len(), 2);
    assert_eq!(
        boundary_evidence
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.payload.as_ref(),
                    Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                        if payload.action_id.as_deref() == Some(action_id.as_str())
                            && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                            && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
                            && payload.evidence_kind == "terminal_input_required"
                )
            })
            .count(),
        1
    );
    assert_eq!(
        boundary_evidence
            .iter()
            .filter(|artifact| {
                matches!(
                    artifact.payload.as_ref(),
                    Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                        if payload.action_id.as_deref() == Some(action_id.as_str())
                            && payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                            && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Passed
                            && payload.evidence_kind == "operator_resume"
                )
            })
            .count(),
        1
    );

    let resumed_snapshot = get_project_snapshot(
        fresh_paused_app.handle().clone(),
        fresh_paused_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after remote resume");
    assert_eq!(resumed_snapshot.resume_history.len(), 1);
    assert_eq!(
        resumed_snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );
    assert_eq!(
        resumed_snapshot.resume_history[0]
            .source_action_id
            .as_deref(),
        Some(action_id.as_str())
    );
    assert!(resumed_snapshot
        .approval_requests
        .iter()
        .any(|approval_request| {
            approval_request.action_id == action_id
                && approval_request.status == OperatorApprovalStatus::Approved
        }));

    let fresh_resumed_app = build_mock_app(create_state(&root));
    let replayed = wait_for_autonomous_run(&fresh_resumed_app, &project_id, |autonomous| {
        let Some(run) = autonomous.run.as_ref() else {
            return false;
        };
        let Some(attempt) = autonomous.attempt.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && attempt.boundary_id.is_none()
            && history_shape(autonomous) == paused_shape
    });
    assert_eq!(history_shape(&replayed), paused_shape);
    assert_eq!(
        replayed
            .run
            .as_ref()
            .map(|run| run.active_unit_id.as_deref()),
        resumed
            .run
            .as_ref()
            .map(|run| run.active_unit_id.as_deref())
    );
    assert_eq!(
        replayed
            .run
            .as_ref()
            .map(|run| run.active_attempt_id.as_deref()),
        resumed
            .run
            .as_ref()
            .map(|run| run.active_attempt_id.as_deref())
    );
    assert_eq!(
        count_autonomous_unit_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(
        count_autonomous_attempt_rows(&repo_root, &project_id, &launched.run.run_id),
        3
    );
    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 3);
    assert_eq!(
        project_store::load_notification_reply_claims(&repo_root, &project_id, Some(&action_id))
            .expect("reload notification reply claims after resume replay")
            .len(),
        3
    );
    assert_eq!(
        get_project_snapshot(
            fresh_resumed_app.handle().clone(),
            fresh_resumed_app.state::<DesktopState>(),
            ProjectIdRequestDto {
                project_id: project_id.clone(),
            },
        )
        .expect("reload project snapshot after resume replay")
        .resume_history
        .len(),
        1
    );

    let stopped = stop_runtime_run(
        fresh_resumed_app.handle().clone(),
        fresh_resumed_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop runtime run after fixture parity proof")
    .expect("runtime run should still exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

#[test]
fn autonomous_fixture_repo_parity_replays_fixture_driven_skill_lifecycle_after_reload() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let app = build_mock_app(create_state(&root));
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime_session = seed_authenticated_runtime(&app, &root, &project_id);
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
        "---\nname: find-skills\ndescription: Discover installable skills.\nuser-invocable: false\n---\n\n# Find Skills\n",
    );
    source.set_file_text(
        "vercel-labs/skills",
        "main",
        "skills/find-skills/guide.md",
        "Use this for discovery.\n",
    );

    let skill_runtime = AutonomousSkillRuntime::with_source_and_cache(
        skill_runtime_config(),
        Arc::new(source.clone()),
        Arc::new(FilesystemAutonomousSkillCacheStore::new(cache_root.clone())),
    );

    let discovered = skill_runtime
        .discover(cadence_desktop_lib::runtime::AutonomousSkillDiscoverRequest {
            query: "find".into(),
            result_limit: Some(5),
            timeout_ms: Some(1_000),
            source_repo: None,
            source_ref: None,
        })
        .expect("fixture discovery should succeed");
    let discovered_source = discovered
        .candidates
        .first()
        .expect("discovery should return one skill candidate")
        .source
        .clone();
    let installed = skill_runtime
        .install(cadence_desktop_lib::runtime::AutonomousSkillInstallRequest {
            source: discovered_source.clone(),
            timeout_ms: Some(1_000),
        })
        .expect("fixture install should succeed");
    let invoked = skill_runtime
        .invoke(cadence_desktop_lib::runtime::AutonomousSkillInvokeRequest {
            source: discovered_source.clone(),
            timeout_ms: Some(1_000),
        })
        .expect("fixture invoke should reuse the Cadence cache");

    assert_eq!(installed.cache_status, cadence_desktop_lib::runtime::AutonomousSkillCacheStatus::Miss);
    assert_eq!(invoked.cache_status, cadence_desktop_lib::runtime::AutonomousSkillCacheStatus::Hit);
    assert_eq!(source.tree_request_count(), 2);
    assert_eq!(source.file_request_count(), 2);
    assert!(Path::new(&installed.cache_directory).starts_with(&cache_root));
    assert!(Path::new(&invoked.cache_directory).starts_with(&cache_root));

    let skill_lines = vec![
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": discovered.candidates[0].skill_id,
                "stage": "discovery",
                "result": "succeeded",
                "detail": "Resolved autonomous skill `find-skills` from the fixture vercel-labs/skills tree.",
                "source": {
                    "repo": discovered_source.repo,
                    "path": discovered_source.path,
                    "reference": discovered_source.reference,
                    "tree_hash": discovered_source.tree_hash,
                }
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": installed.skill_id,
                "stage": "install",
                "result": "succeeded",
                "detail": "Installed autonomous skill `find-skills` from the Cadence-owned fixture cache.",
                "source": {
                    "repo": installed.source.repo,
                    "path": installed.source.path,
                    "reference": installed.source.reference,
                    "tree_hash": installed.source.tree_hash,
                },
                "cache_status": "miss"
            })
        ),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "skill",
                "skill_id": invoked.skill_id,
                "stage": "invoke",
                "result": "succeeded",
                "detail": "Invoked autonomous skill `find-skills` from the Cadence-owned fixture cache.",
                "source": {
                    "repo": invoked.source.repo,
                    "path": invoked.source.path,
                    "reference": invoked.source.reference,
                    "tree_hash": invoked.source.tree_hash,
                },
                "cache_status": "hit"
            })
        ),
    ];

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-skill-fixture-parity",
        runtime_session
            .session_id
            .as_deref()
            .expect("authenticated runtime session id"),
        runtime_session.flow_id.as_deref(),
        &runtime_shell::script_print_lines_and_sleep(&skill_lines, 3),
    );

    let observed = wait_for_autonomous_run(&app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };

        run.run_id == launched.run.run_id
            && autonomous_state.history.first().is_some_and(|entry| {
                entry
                    .artifacts
                    .iter()
                    .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
                    .count()
                    == 3
            })
    });
    let observed_skill_count = observed
        .history
        .first()
        .expect("observed autonomous history entry")
        .artifacts
        .iter()
        .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
        .count();
    assert_eq!(observed_skill_count, 3);

    let durable = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load durable autonomous run after fixture skill story")
        .expect("durable autonomous run should exist after fixture skill story");
    let skill_artifacts = durable
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
        .collect::<Vec<_>>();
    assert_eq!(skill_artifacts.len(), 3);
    assert!(skill_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Discovery
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.skill_id == "find-skills"
                    && payload.source.repo == "vercel-labs/skills"
                    && payload.source.path == "skills/find-skills"
                    && payload.source.reference == "main"
                    && payload.source.tree_hash == "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
                    && payload.cache.status.is_none()
        )
    }));
    assert!(skill_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Install
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.cache.status
                        == Some(project_store::AutonomousSkillCacheStatusRecord::Miss)
        )
    }));
    assert!(skill_artifacts.iter().any(|artifact| {
        matches!(
            artifact.payload.as_ref(),
            Some(project_store::AutonomousArtifactPayloadRecord::SkillLifecycle(payload))
                if payload.stage == project_store::AutonomousSkillLifecycleStageRecord::Invoke
                    && payload.result == project_store::AutonomousSkillLifecycleResultRecord::Succeeded
                    && payload.cache.status
                        == Some(project_store::AutonomousSkillCacheStatusRecord::Hit)
        )
    }));

    let payload_jsons = load_skill_payload_jsons(&repo_root);
    assert_eq!(payload_jsons.len(), 3);
    assert!(payload_jsons.iter().all(|payload| !payload.contains("# Find Skills")));
    assert!(payload_jsons.iter().all(|payload| !payload.contains("Use this for discovery.")));
    assert!(payload_jsons.iter().all(|payload| !payload.contains("SKILL.md")));

    let fresh_app = build_mock_app(create_state(&root));
    let fresh_runtime = seed_authenticated_runtime(&fresh_app, &root, &project_id);
    let reloaded = wait_for_autonomous_run(&fresh_app, &project_id, |autonomous_state| {
        let Some(run) = autonomous_state.run.as_ref() else {
            return false;
        };
        run.run_id == launched.run.run_id
            && autonomous_state.history.first().is_some_and(|entry| {
                entry
                    .artifacts
                    .iter()
                    .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
                    .count()
                    == 3
            })
    });
    assert_eq!(
        reloaded
            .history
            .first()
            .expect("reloaded autonomous history entry")
            .artifacts
            .iter()
            .filter(|artifact| artifact.artifact_kind == "skill_lifecycle")
            .count(),
        3
    );

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &fresh_app,
        &project_id,
        &repo_root,
        &fresh_runtime,
        &launched.run.run_id,
        vec![RuntimeStreamItemKind::Skill, RuntimeStreamItemKind::Complete],
        channel,
    );

    let items = collect_until_terminal(receiver);
    assert_monotonic_sequences(&items, &launched.run.run_id);
    assert_eq!(
        items.iter().map(|item| item.kind.clone()).collect::<Vec<_>>(),
        vec![
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Skill,
            RuntimeStreamItemKind::Complete,
        ],
        "unexpected replayed fixture skill items: {items:?}"
    );

    assert_eq!(items[0].skill_id.as_deref(), Some("find-skills"));
    assert_eq!(
        items[0].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Discovery)
    );
    assert_eq!(
        items[0].skill_result,
        Some(AutonomousSkillLifecycleResultDto::Succeeded)
    );
    assert_eq!(items[0].skill_cache_status, None);
    assert_eq!(
        items[0].detail.as_deref(),
        Some("Resolved autonomous skill `find-skills` from the fixture vercel-labs/skills tree.")
    );

    assert_eq!(items[1].skill_id.as_deref(), Some("find-skills"));
    assert_eq!(
        items[1].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Install)
    );
    assert_eq!(
        items[1].skill_result,
        Some(AutonomousSkillLifecycleResultDto::Succeeded)
    );
    assert_eq!(
        items[1].skill_cache_status,
        Some(AutonomousSkillCacheStatusDto::Miss)
    );
    assert_eq!(
        items[1].detail.as_deref(),
        Some("Installed autonomous skill `find-skills` from the Cadence-owned fixture cache.")
    );

    assert_eq!(items[2].skill_id.as_deref(), Some("find-skills"));
    assert_eq!(
        items[2].skill_stage,
        Some(AutonomousSkillLifecycleStageDto::Invoke)
    );
    assert_eq!(
        items[2].skill_result,
        Some(AutonomousSkillLifecycleResultDto::Succeeded)
    );
    assert_eq!(
        items[2].skill_cache_status,
        Some(AutonomousSkillCacheStatusDto::Hit)
    );
    assert_eq!(
        items[2].detail.as_deref(),
        Some("Invoked autonomous skill `find-skills` from the Cadence-owned fixture cache.")
    );
    assert!(items[2].skill_diagnostic.is_none());

    let final_runtime = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.run_id == launched.run.run_id
            && runtime_run.status == RuntimeRunStatusDto::Stopped
    });
    assert_eq!(final_runtime.status, RuntimeRunStatusDto::Stopped);
}
