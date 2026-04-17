use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use cadence_desktop_lib::{
    auth::{persist_openai_codex_session, StoredOpenAiCodexSession},
    commands::{
        apply_workflow_transition::apply_workflow_transition,
        get_project_snapshot::get_project_snapshot, get_runtime_run::get_runtime_run,
        get_runtime_session::get_runtime_session, resolve_operator_action::resolve_operator_action,
        resume_operator_run::resume_operator_run, start_runtime_run::start_runtime_run,
        start_runtime_session::start_runtime_session, stop_runtime_run::stop_runtime_run,
        submit_notification_reply::submit_notification_reply, ApplyWorkflowTransitionRequestDto,
        GetRuntimeRunRequestDto, NotificationDispatchStatusDto, NotificationReplyClaimStatusDto,
        OperatorApprovalStatus, PhaseStatus, PhaseStep, ProjectIdRequestDto, ProjectUpdateReason,
        ProjectUpdatedPayloadDto, ResolveOperatorActionRequestDto, ResumeHistoryStatus,
        ResumeOperatorRunRequestDto, RuntimeAuthPhase, RuntimeRunCheckpointKindDto, RuntimeRunDto,
        RuntimeRunStatusDto, RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto,
        RuntimeUpdatedPayloadDto, StartRuntimeRunRequestDto, StopRuntimeRunRequestDto,
        SubmitNotificationReplyRequestDto, WorkflowAutomaticDispatchStatusDto,
        PROJECT_UPDATED_EVENT, RUNTIME_RUN_UPDATED_EVENT, RUNTIME_UPDATED_EVENT,
    },
    configure_builder_with_state,
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    runtime::{
        launch_detached_runtime_supervisor,
        protocol::{
            SupervisorControlRequest, SupervisorControlResponse, SupervisorLiveEventPayload,
        },
        RuntimeSupervisorLaunchRequest,
    },
    state::DesktopState,
};
use serde_json::json;
use tauri::{Listener, Manager};
use tempfile::TempDir;

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

#[derive(Clone, Default)]
struct EventRecorder {
    project_updates: Arc<Mutex<Vec<ProjectUpdatedPayloadDto>>>,
    runtime_updates: Arc<Mutex<Vec<RuntimeUpdatedPayloadDto>>>,
    runtime_run_updates: Arc<Mutex<Vec<RuntimeRunUpdatedPayloadDto>>>,
}

impl EventRecorder {
    fn clear(&self) {
        self.project_updates
            .lock()
            .expect("project updates lock")
            .clear();
        self.runtime_updates
            .lock()
            .expect("runtime updates lock")
            .clear();
        self.runtime_run_updates
            .lock()
            .expect("runtime run updates lock")
            .clear();
    }

    fn project_update_count(&self) -> usize {
        self.project_updates
            .lock()
            .expect("project updates lock")
            .len()
    }

    fn latest_project_update(&self) -> Option<ProjectUpdatedPayloadDto> {
        self.project_updates
            .lock()
            .expect("project updates lock")
            .last()
            .cloned()
    }

    fn runtime_update_count(&self) -> usize {
        self.runtime_updates
            .lock()
            .expect("runtime updates lock")
            .len()
    }

    fn runtime_run_update_count(&self) -> usize {
        self.runtime_run_updates
            .lock()
            .expect("runtime run updates lock")
            .len()
    }
}

fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

fn create_state(root: &TempDir) -> (DesktopState, PathBuf, PathBuf) {
    let registry_path = root.path().join("app-data").join("project-registry.json");
    let auth_store_path = root.path().join("app-data").join("openai-auth.json");
    (
        DesktopState::default()
            .with_registry_file_override(registry_path.clone())
            .with_auth_store_file_override(auth_store_path.clone())
            .with_runtime_supervisor_binary_override(supervisor_binary_path()),
        registry_path,
        auth_store_path,
    )
}

fn attach_event_recorders(app: &tauri::App<tauri::test::MockRuntime>) -> EventRecorder {
    let recorder = EventRecorder::default();

    let project_updates = Arc::clone(&recorder.project_updates);
    app.listen(PROJECT_UPDATED_EVENT, move |event| {
        let payload: ProjectUpdatedPayloadDto = serde_json::from_str(event.payload())
            .expect("project updated payload should deserialize");
        project_updates
            .lock()
            .expect("project updates lock")
            .push(payload);
    });

    let runtime_updates = Arc::clone(&recorder.runtime_updates);
    app.listen(RUNTIME_UPDATED_EVENT, move |event| {
        let payload: RuntimeUpdatedPayloadDto = serde_json::from_str(event.payload())
            .expect("runtime updated payload should deserialize");
        runtime_updates
            .lock()
            .expect("runtime updates lock")
            .push(payload);
    });

    let runtime_run_updates = Arc::clone(&recorder.runtime_run_updates);
    app.listen(RUNTIME_RUN_UPDATED_EVENT, move |event| {
        let payload: RuntimeRunUpdatedPayloadDto = serde_json::from_str(event.payload())
            .expect("runtime run updated payload should deserialize");
        runtime_run_updates
            .lock()
            .expect("runtime run updates lock")
            .push(payload);
    });

    recorder
}

fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cadence-runtime-supervisor"))
}

fn jwt_with_account_id(account_id: &str) -> String {
    let header = URL_SAFE_NO_PAD.encode(r#"{"alg":"none","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(
        json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": account_id,
            }
        })
        .to_string(),
    );
    format!("{header}.{payload}.")
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
            access_token: jwt_with_account_id("acct-1"),
            refresh_token: "refresh-1".into(),
            expires_at: current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
            updated_at: "2026-04-13T14:11:59Z".into(),
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

fn seed_gate_linked_workflow_with_auto_continuation(
    repo_root: &Path,
    project_id: &str,
    action_type: &str,
) {
    project_store::upsert_workflow_graph(
        repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "plan".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Plan".into(),
                    description: "Plan phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 2,
                    completed_tasks: 1,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 4,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Verify),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "plan".into(),
                    to_node_id: "execute".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("execution_gate".into()),
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: None,
                },
            ],
            gates: vec![project_store::WorkflowGateMetadataRecord {
                node_id: "execute".into(),
                gate_key: "execution_gate".into(),
                gate_state: project_store::WorkflowGateState::Pending,
                action_type: Some(action_type.into()),
                title: Some("Approve execution".into()),
                detail: Some("Operator approval required.".into()),
                decision_context: None,
            }],
        },
    )
    .expect("seed gate-linked workflow graph with auto continuation");
}

fn seed_gate_linked_workflow_with_blocked_auto_continuation(
    repo_root: &Path,
    project_id: &str,
    action_type: &str,
    blocked_action_type: &str,
) {
    project_store::upsert_workflow_graph(
        repo_root,
        project_id,
        &project_store::WorkflowGraphUpsertRecord {
            nodes: vec![
                project_store::WorkflowGraphNodeRecord {
                    node_id: "plan".into(),
                    phase_id: 1,
                    sort_order: 1,
                    name: "Plan".into(),
                    description: "Plan phase".into(),
                    status: PhaseStatus::Active,
                    current_step: Some(PhaseStep::Plan),
                    task_count: 2,
                    completed_tasks: 1,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "execute".into(),
                    phase_id: 2,
                    sort_order: 2,
                    name: "Execute".into(),
                    description: "Execute phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Execute),
                    task_count: 4,
                    completed_tasks: 0,
                    summary: None,
                },
                project_store::WorkflowGraphNodeRecord {
                    node_id: "verify".into(),
                    phase_id: 3,
                    sort_order: 3,
                    name: "Verify".into(),
                    description: "Verify phase".into(),
                    status: PhaseStatus::Pending,
                    current_step: Some(PhaseStep::Verify),
                    task_count: 1,
                    completed_tasks: 0,
                    summary: None,
                },
            ],
            edges: vec![
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "plan".into(),
                    to_node_id: "execute".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("execution_gate".into()),
                },
                project_store::WorkflowGraphEdgeRecord {
                    from_node_id: "execute".into(),
                    to_node_id: "verify".into(),
                    transition_kind: "advance".into(),
                    gate_requirement: Some("verify_gate".into()),
                },
            ],
            gates: vec![
                project_store::WorkflowGateMetadataRecord {
                    node_id: "execute".into(),
                    gate_key: "execution_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some(action_type.into()),
                    title: Some("Approve execution".into()),
                    detail: Some("Operator approval required.".into()),
                    decision_context: None,
                },
                project_store::WorkflowGateMetadataRecord {
                    node_id: "verify".into(),
                    gate_key: "verify_gate".into(),
                    gate_state: project_store::WorkflowGateState::Pending,
                    action_type: Some(blocked_action_type.into()),
                    title: Some("Approve verify".into()),
                    detail: Some(
                        "Operator approval required before verify continuation can proceed.".into(),
                    ),
                    decision_context: None,
                },
            ],
        },
    )
    .expect("seed gate-linked workflow graph with blocked auto continuation");
}

fn seed_planning_lifecycle_workflow(
    repo_root: &Path,
    project_id: &str,
    pause_at_roadmap_gate: bool,
) {
    let roadmap_gate_state = if pause_at_roadmap_gate {
        project_store::WorkflowGateState::Pending
    } else {
        project_store::WorkflowGateState::Satisfied
    };

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
                    gate_requirement: Some("roadmap_gate".into()),
                },
            ],
            gates: vec![project_store::WorkflowGateMetadataRecord {
                node_id: "roadmap".into(),
                gate_key: "roadmap_gate".into(),
                gate_state: roadmap_gate_state,
                action_type: Some("approve_roadmap".into()),
                title: Some("Approve roadmap".into()),
                detail: Some("Review roadmap draft before scheduling.".into()),
                decision_context: None,
            }],
        },
    )
    .expect("seed planning lifecycle workflow");
}

fn replay_transition_request(
    project_id: &str,
    transition: &cadence_desktop_lib::commands::WorkflowTransitionEventDto,
) -> ApplyWorkflowTransitionRequestDto {
    ApplyWorkflowTransitionRequestDto {
        project_id: project_id.into(),
        transition_id: transition.transition_id.clone(),
        causal_transition_id: transition.causal_transition_id.clone(),
        from_node_id: transition.from_node_id.clone(),
        to_node_id: transition.to_node_id.clone(),
        transition_kind: transition.transition_kind.clone(),
        gate_decision: "not_applicable".into(),
        gate_decision_context: None,
        gate_updates: Vec::new(),
        occurred_at: transition.created_at.clone(),
    }
}

fn wait_for_runtime_run(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    predicate: impl Fn(&RuntimeRunDto) -> bool,
) -> RuntimeRunDto {
    let deadline = Instant::now() + Duration::from_secs(6);

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

fn count_runtime_run_rows(repo_root: &Path) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row("SELECT COUNT(*) FROM runtime_runs", [], |row| row.get(0))
        .expect("count runtime runs")
}

fn count_workflow_transition_rows(repo_root: &Path, project_id: &str) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM workflow_transition_events WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count workflow transition rows")
}

fn count_workflow_handoff_rows(repo_root: &Path, project_id: &str) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM workflow_handoff_packages WHERE project_id = ?1",
            [project_id],
            |row| row.get(0),
        )
        .expect("count workflow handoff rows")
}

fn count_pending_gate_approval_rows(repo_root: &Path, project_id: &str, gate_key: &str) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM operator_approvals WHERE project_id = ?1 AND gate_key = ?2 AND status = 'pending'",
            [project_id, gate_key],
            |row| row.get(0),
        )
        .expect("count pending gate approval rows")
}

fn count_operator_approval_rows_for_action(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM operator_approvals WHERE project_id = ?1 AND action_id = ?2",
            [project_id, action_id],
            |row| row.get(0),
        )
        .expect("count operator approval rows for action")
}

fn upsert_notification_route(repo_root: &Path, project_id: &str, route_id: &str) {
    project_store::upsert_notification_route(
        repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.into(),
            route_id: route_id.into(),
            route_kind: "discord".into(),
            route_target: "discord:ops-room".into(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-16T14:59:59Z".into(),
        },
    )
    .expect("upsert notification route");
}

fn load_notification_dispatches_for_action(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
) -> Vec<project_store::NotificationDispatchRecord> {
    project_store::load_notification_dispatches(repo_root, project_id, Some(action_id))
        .expect("load notification dispatches for action")
}

fn seed_unreachable_runtime_run(repo_root: &Path, project_id: &str, run_id: &str) {
    project_store::upsert_runtime_run(
        repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Running,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "127.0.0.1:9".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Unknown,
                },
                started_at: "2026-04-15T19:00:00Z".into(),
                last_heartbeat_at: Some("2026-04-15T19:00:10Z".into()),
                stopped_at: None,
                last_error: None,
                updated_at: "2026-04-15T19:00:10Z".into(),
            },
            checkpoint: Some(project_store::RuntimeRunCheckpointRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                sequence: 1,
                kind: project_store::RuntimeRunCheckpointKind::Bootstrap,
                summary: "Supervisor boot recorded.".into(),
                created_at: "2026-04-15T19:00:10Z".into(),
            }),
        },
    )
    .expect("seed unreachable runtime run");
}

fn seed_failed_runtime_run(repo_root: &Path, project_id: &str, run_id: &str) {
    project_store::upsert_runtime_run(
        repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::RuntimeRunStatus::Failed,
                transport: project_store::RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "launch-pending".into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Unknown,
                },
                started_at: "2026-04-15T19:00:00Z".into(),
                last_heartbeat_at: None,
                stopped_at: Some("2026-04-15T19:00:11Z".into()),
                last_error: Some(project_store::RuntimeRunDiagnosticRecord {
                    code: "runtime_supervisor_exit_nonzero".into(),
                    message: "The detached runtime supervisor exited with status 17.".into(),
                }),
                updated_at: "2026-04-15T19:00:11Z".into(),
            },
            checkpoint: None,
        },
    )
    .expect("seed failed runtime run");
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

fn attach_reader(endpoint: &str, request: SupervisorControlRequest) -> BufReader<TcpStream> {
    let mut stream = TcpStream::connect(endpoint).expect("connect attach reader");
    stream
        .set_read_timeout(Some(Duration::from_millis(150)))
        .expect("set read timeout");
    serde_json::to_writer(&mut stream, &request).expect("serialize attach request");
    stream.write_all(b"\n").expect("write attach newline");
    stream.flush().expect("flush attach request");
    BufReader::new(stream)
}

fn read_supervisor_response(
    reader: &mut BufReader<TcpStream>,
) -> Option<SupervisorControlResponse> {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => None,
        Ok(_) => {
            Some(serde_json::from_str(line.trim()).expect("decode supervisor control response"))
        }
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ) =>
        {
            None
        }
        Err(error) => panic!("failed to read supervisor response: {error}"),
    }
}

fn expect_attach_ack(response: Option<SupervisorControlResponse>) -> SupervisorControlResponse {
    match response {
        Some(response @ SupervisorControlResponse::Attached { .. }) => response,
        other => panic!("expected attach ack, got {other:?}"),
    }
}

fn read_event_frames(
    reader: &mut BufReader<TcpStream>,
    replayed_count: u32,
) -> Vec<SupervisorControlResponse> {
    let mut frames = Vec::new();
    for _ in 0..replayed_count {
        let Some(response) = read_supervisor_response(reader) else {
            panic!("expected replay event frame");
        };
        frames.push(response);
    }
    frames
}

#[test]
fn get_runtime_run_returns_none_when_selected_project_has_no_durable_run() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    let runtime_run = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto { project_id },
    )
    .expect("get runtime run should succeed");

    assert!(runtime_run.is_none());
}

#[test]
fn get_runtime_run_fails_closed_for_malformed_durable_rows_without_projection_event_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_unreachable_runtime_run(&repo_root, &project_id, "run-corrupt");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .execute_batch("PRAGMA ignore_check_constraints = 1;")
        .expect("disable check constraints");
    connection
        .execute(
            "UPDATE runtime_runs SET status = 'bogus_status' WHERE project_id = ?1",
            [&project_id],
        )
        .expect("corrupt runtime run status");

    recorder.clear();
    let error = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect_err("malformed runtime run rows should fail closed");

    assert_eq!(error.code, "runtime_run_decode_failed");
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);
}

#[test]
fn start_runtime_run_requires_authenticated_runtime_session() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    let error = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto { project_id },
    )
    .expect_err("start runtime run should require auth binding");

    assert_eq!(error.code, "runtime_run_auth_required");
}

#[test]
fn start_runtime_run_reconnects_existing_run_without_duplicate_launch_or_auth_event_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    recorder.clear();

    let first = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start runtime run");
    assert_eq!(first.project_id, project_id);

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, first.run_id);

    let second = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("second start should reconnect");
    assert_eq!(second.run_id, first.run_id);
    assert_eq!(count_runtime_run_rows(&repo_root), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert!(recorder.runtime_run_update_count() >= 1);

    let auth_runtime = get_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime session after run start");
    assert_eq!(auth_runtime.phase, RuntimeAuthPhase::Authenticated);

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: first.run_id,
        },
    )
    .expect("stop runtime run should succeed")
    .expect("stopped runtime run should exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

#[test]
fn get_runtime_run_recovers_truthful_running_state_after_fresh_host_reload() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, _repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start runtime run");

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);

    let recovered = wait_for_runtime_run(&fresh_app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(recovered.run_id, launched.run_id);

    let stopped = stop_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop recovered runtime run")
    .expect("recovered runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

#[test]
fn get_runtime_run_recovers_stale_unreachable_state_once_after_fresh_host_reload() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    seed_unreachable_runtime_run(&repo_root, &project_id, "run-unreachable");

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let recorder = attach_event_recorders(&fresh_app);

    let first = get_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run after fresh-host restart")
    .expect("runtime run should exist after restart");
    assert_eq!(first.run_id, "run-unreachable");
    assert_eq!(first.status, RuntimeRunStatusDto::Stale);
    assert_eq!(
        first.transport.liveness,
        RuntimeRunTransportLivenessDto::Unreachable
    );
    assert_eq!(
        first.last_error_code.as_deref(),
        Some("runtime_supervisor_connect_failed")
    );
    assert_eq!(first.last_checkpoint_sequence, 1);
    assert_eq!(first.checkpoints.len(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 1);

    let runtime_session = get_runtime_session(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime session after stale recovery");
    assert_eq!(runtime_session.phase, RuntimeAuthPhase::Authenticated);
    assert_eq!(recorder.runtime_update_count(), 0);

    let second = get_runtime_run(
        fresh_app.handle().clone(),
        fresh_app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("reload stale runtime run projection")
    .expect("runtime run should remain durable after stale recovery");

    assert_eq!(second.run_id, first.run_id);
    assert_eq!(second.status, first.status);
    assert_eq!(second.transport.liveness, first.transport.liveness);
    assert_eq!(second.last_error_code, first.last_error_code);
    assert_eq!(second.last_checkpoint_sequence, first.last_checkpoint_sequence);
    assert_eq!(second.checkpoints, first.checkpoints);
    assert_eq!(
        second.updated_at, first.updated_at,
        "unchanged stale projections should not rewrite durable runtime rows"
    );
    assert_eq!(
        recorder.runtime_run_update_count(),
        1,
        "unchanged stale recovery should not emit duplicate runtime-run updates"
    );
}

#[test]
fn apply_workflow_transition_gate_pause_returns_skipped_diagnostics_and_truthful_project_update() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_blocked_auto_continuation(
        &repo_root,
        &project_id,
        "approve_execution",
        "approve_verify",
    );
    upsert_notification_route(&repo_root, &project_id, "route-discord");

    let request = ApplyWorkflowTransitionRequestDto {
        project_id: project_id.clone(),
        transition_id: "transition-gate-pause-1".into(),
        causal_transition_id: None,
        from_node_id: "plan".into(),
        to_node_id: "execute".into(),
        transition_kind: "advance".into(),
        gate_decision: "approved".into(),
        gate_decision_context: Some("approved by operator".into()),
        gate_updates: vec![
            cadence_desktop_lib::commands::WorkflowTransitionGateUpdateRequestDto {
                gate_key: "execution_gate".into(),
                gate_state: "satisfied".into(),
                decision_context: Some("approved by operator".into()),
            },
        ],
        occurred_at: "2026-04-16T15:00:00Z".into(),
    };

    recorder.clear();
    let applied = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        request.clone(),
    )
    .expect("apply transition should persist and return skipped automatic-dispatch diagnostics");

    assert_eq!(
        applied.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        applied.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    let applied_message = applied
        .automatic_dispatch
        .message
        .as_deref()
        .expect("skipped automatic-dispatch outcome should include diagnostics");
    assert!(
        applied_message.contains("Persisted pending operator approval"),
        "expected persisted-approval diagnostics, got {applied_message}"
    );
    assert!(
        applied.automatic_dispatch.transition_event.is_none(),
        "skipped automatic-dispatch outcome must not fabricate a transition event"
    );
    assert!(
        applied.automatic_dispatch.handoff_package.is_none(),
        "skipped automatic-dispatch outcome must not fabricate a handoff package"
    );

    assert_eq!(recorder.project_update_count(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after gate pause");
    let pending_verify_approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| {
            approval.gate_key.as_deref() == Some("verify_gate")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .expect("expected pending verify-gate approval persisted from skipped automatic dispatch");
    assert_eq!(
        pending_verify_approval.gate_node_id.as_deref(),
        Some("verify")
    );
    assert_eq!(
        pending_verify_approval.transition_from_node_id.as_deref(),
        Some("execute")
    );
    assert_eq!(
        pending_verify_approval.transition_to_node_id.as_deref(),
        Some("verify")
    );
    assert_eq!(
        pending_verify_approval.transition_kind.as_deref(),
        Some("advance")
    );
    assert_eq!(pending_verify_approval.action_type, "approve_verify");

    let persisted_action_id = pending_verify_approval.action_id.clone();
    assert!(
        applied_message.contains(persisted_action_id.as_str()),
        "expected skipped diagnostics to include deterministic persisted action id, got {applied_message}"
    );

    let initial_dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &persisted_action_id);
    assert_eq!(initial_dispatches.len(), 1);
    assert_eq!(
        initial_dispatches[0].status,
        project_store::NotificationDispatchStatus::Pending
    );

    let initial_events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after apply gate pause");
    assert_eq!(initial_events.len(), 1);
    assert_eq!(initial_events[0].transition_id, request.transition_id);
    assert_eq!(initial_events[0].from_node_id, "plan");
    assert_eq!(initial_events[0].to_node_id, "execute");

    recorder.clear();
    let replayed =
        apply_workflow_transition(app.handle().clone(), app.state::<DesktopState>(), request)
            .expect("replayed apply transition should remain idempotent with skipped diagnostics");

    assert_eq!(
        replayed.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        replayed.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    assert!(replayed.automatic_dispatch.transition_event.is_none());
    assert!(replayed.automatic_dispatch.handoff_package.is_none());

    let replayed_message = replayed
        .automatic_dispatch
        .message
        .as_deref()
        .expect("replayed skipped diagnostics should include message");
    assert!(
        replayed_message.contains(persisted_action_id.as_str()),
        "expected replayed skipped diagnostics to keep deterministic action id, got {replayed_message}"
    );

    assert_eq!(recorder.project_update_count(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);

    let replay_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after replayed apply gate pause");
    let replay_pending_verify: Vec<_> = replay_snapshot
        .approval_requests
        .iter()
        .filter(|approval| {
            approval.gate_key.as_deref() == Some("verify_gate")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .collect();
    assert_eq!(replay_pending_verify.len(), 1);
    assert_eq!(replay_pending_verify[0].action_id, persisted_action_id);

    let replay_dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &persisted_action_id);
    assert_eq!(replay_dispatches.len(), 1);
    assert_eq!(replay_dispatches[0].id, initial_dispatches[0].id);

    let replay_events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after replayed apply gate pause");
    assert_eq!(replay_events, initial_events);
}

#[test]
fn gate_linked_resume_gate_pause_returns_skipped_diagnostics_without_runtime_event_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_blocked_auto_continuation(
        &repo_root,
        &project_id,
        "approve_execution",
        "approve_verify",
    );

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T15:10:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked operator action");

    recorder.clear();

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect("resume should apply gate transition and persist skipped auto-dispatch diagnostics");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);
    let auto_dispatch = resumed
        .automatic_dispatch
        .expect("gate-linked resume should include automatic-dispatch diagnostics");
    assert_eq!(
        auto_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        auto_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    let auto_dispatch_message = auto_dispatch
        .message
        .as_deref()
        .expect("skipped diagnostics should include message");
    assert!(
        auto_dispatch_message.contains("Persisted pending operator approval"),
        "expected persisted-approval diagnostics, got {auto_dispatch_message}"
    );
    assert!(
        auto_dispatch.transition_event.is_none(),
        "skipped auto-dispatch diagnostics must not fabricate transition payloads"
    );
    assert!(
        auto_dispatch.handoff_package.is_none(),
        "skipped auto-dispatch diagnostics must not fabricate handoff payloads"
    );

    assert_eq!(recorder.project_update_count(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);

    let project_update = recorder
        .latest_project_update()
        .expect("expected project:updated payload after resume gate pause");
    assert_eq!(project_update.project.id, project_id);
    assert_eq!(project_update.reason, ProjectUpdateReason::MetadataChanged);
    assert_eq!(project_update.project.active_phase, 2);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after resume gate pause");

    let pending_verify_approval = snapshot
        .approval_requests
        .iter()
        .find(|approval| {
            approval.gate_key.as_deref() == Some("verify_gate")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .expect("expected pending verify-gate approval persisted from resume auto-dispatch skip");
    assert_eq!(
        pending_verify_approval.gate_node_id.as_deref(),
        Some("verify")
    );
    assert_eq!(
        pending_verify_approval.transition_from_node_id.as_deref(),
        Some("execute")
    );
    assert_eq!(
        pending_verify_approval.transition_to_node_id.as_deref(),
        Some("verify")
    );
    assert_eq!(
        pending_verify_approval.transition_kind.as_deref(),
        Some("advance")
    );

    assert!(
        auto_dispatch_message.contains(pending_verify_approval.action_id.as_str()),
        "expected skipped diagnostics to include persisted action id, got {auto_dispatch_message}"
    );

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after resume gate pause");
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].from_node_id, "plan");
    assert_eq!(events[0].to_node_id, "execute");
    assert!(
        events[0].transition_id.starts_with("resume:"),
        "expected deterministic resume transition id, got {}",
        events[0].transition_id
    );
}

#[test]
fn gate_linked_resume_auto_dispatch_emits_project_update_without_runtime_event_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_auto_continuation(&repo_root, &project_id, "approve_execution");

    let pending = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T15:00:00Z",
    )
    .expect("persist gate-linked approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: pending.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Execution gate approved by operator.".into()),
        },
    )
    .expect("approve gate-linked operator action");

    recorder.clear();

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending.action_id,
            user_answer: None,
        },
    )
    .expect("resume should auto-dispatch continuation without manual apply command");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);
    assert_eq!(recorder.project_update_count(), 1);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert_eq!(recorder.runtime_run_update_count(), 0);

    let project_update = recorder
        .latest_project_update()
        .expect("expected project:updated payload after resume auto-dispatch");
    assert_eq!(project_update.project.id, project_id);
    assert_eq!(project_update.reason, ProjectUpdateReason::MetadataChanged);
    assert_eq!(project_update.project.active_phase, 3);
    assert_eq!(project_update.project.completed_phases, 2);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after gate-linked auto-dispatch");
    assert_eq!(events.len(), 2);

    let primary_event = events
        .iter()
        .find(|event| event.from_node_id == "plan" && event.to_node_id == "execute")
        .expect("primary gate-linked transition event");
    let auto_event = events
        .iter()
        .find(|event| event.from_node_id == "execute" && event.to_node_id == "verify")
        .expect("automatic continuation transition event");

    assert!(
        primary_event.transition_id.starts_with("resume:"),
        "expected deterministic resume transition id, got {}",
        primary_event.transition_id
    );
    assert!(
        auto_event.transition_id.starts_with("auto:"),
        "expected deterministic auto transition id, got {}",
        auto_event.transition_id
    );
    assert_eq!(
        auto_event.causal_transition_id.as_deref(),
        Some(primary_event.transition_id.as_str())
    );

    let persisted_auto_package = project_store::load_workflow_handoff_package(
        &repo_root,
        &project_id,
        &auto_event.transition_id,
    )
    .expect("load persisted handoff package for runtime bridge auto transition")
    .expect("handoff package row should exist for runtime bridge auto transition");
    let persisted_payload: serde_json::Value =
        serde_json::from_str(&persisted_auto_package.package_payload)
            .expect("decode runtime bridge auto handoff payload");
    assert_eq!(
        persisted_payload["triggerTransition"]["transitionId"],
        auto_event.transition_id
    );
    assert_eq!(
        persisted_payload["triggerTransition"]["causalTransitionId"],
        primary_event.transition_id
    );

    let reloaded_events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("reload transition events after gate-linked auto-dispatch");
    assert_eq!(events, reloaded_events);
}

#[test]
fn runtime_action_required_persistence_enqueues_notification_dispatches_once_per_route() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    upsert_notification_route(&repo_root, &project_id, "route-discord");
    seed_unreachable_runtime_run(&repo_root, &project_id, "run-dispatch");

    let first = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: "run-dispatch".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T19:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T19:00:10Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-16T20:40:00Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    assert_eq!(
        first.notification_dispatch_outcome.status,
        project_store::NotificationDispatchEnqueueStatus::Enqueued
    );
    assert_eq!(first.notification_dispatch_outcome.dispatch_count, 1);

    let action_id = first.approval_request.action_id.clone();
    let first_dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &action_id);
    assert_eq!(first_dispatches.len(), 1);
    assert_eq!(
        first_dispatches[0].status,
        project_store::NotificationDispatchStatus::Pending
    );

    let second = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: "run-dispatch".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T19:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T19:00:10Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary:
                "Detached runtime blocked on terminal input and is awaiting operator approval."
                    .into(),
            created_at: "2026-04-16T20:40:01Z".into(),
        },
    )
    .expect("replay runtime action-required approval");

    let second_dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &action_id);
    assert_eq!(second_dispatches.len(), 1);
    assert_eq!(second_dispatches[0].id, first_dispatches[0].id);
    assert_eq!(
        second.notification_dispatch_outcome.dispatch_count,
        first.notification_dispatch_outcome.dispatch_count
    );
}

#[test]
fn submit_notification_reply_first_wins_and_rejects_forged_and_duplicate_replies() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_auto_continuation(&repo_root, &project_id, "approve_execution");
    upsert_notification_route(&repo_root, &project_id, "route-discord");

    let primary = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T20:41:00Z",
    )
    .expect("persist primary pending approval");
    let secondary = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-2",
        Some("flow-2"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T20:41:01Z",
    )
    .expect("persist secondary pending approval");

    let primary_dispatch =
        load_notification_dispatches_for_action(&repo_root, &project_id, &primary.action_id)
            .into_iter()
            .next()
            .expect("primary dispatch row should exist");
    let secondary_dispatch =
        load_notification_dispatches_for_action(&repo_root, &project_id, &secondary.action_id)
            .into_iter()
            .next()
            .expect("secondary dispatch row should exist");

    let first = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: primary.action_id.clone(),
            route_id: primary_dispatch.route_id.clone(),
            correlation_key: primary_dispatch.correlation_key.clone(),
            responder_id: Some("operator-a".into()),
            reply_text: "Execution approved.".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:41:05Z".into(),
        },
    )
    .expect("first reply should claim, resolve, and resume");

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

    let forged_error = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: secondary.action_id.clone(),
            route_id: secondary_dispatch.route_id,
            correlation_key: primary_dispatch.correlation_key.clone(),
            responder_id: Some("operator-b".into()),
            reply_text: "Forged correlation".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:41:06Z".into(),
        },
    )
    .expect_err("forged correlation must fail closed");
    assert_eq!(forged_error.code, "notification_reply_correlation_invalid");

    let duplicate_error = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: primary.action_id.clone(),
            route_id: primary_dispatch.route_id,
            correlation_key: first.dispatch.correlation_key.clone(),
            responder_id: Some("operator-c".into()),
            reply_text: "Duplicate answer".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:41:07Z".into(),
        },
    )
    .expect_err("duplicate reply after winner should fail closed");
    assert_eq!(duplicate_error.code, "notification_reply_already_claimed");

    let claims = project_store::load_notification_reply_claims(
        &repo_root,
        &project_id,
        Some(&primary.action_id),
    )
    .expect("load primary reply claims");
    assert_eq!(claims.len(), 2);
    assert!(claims
        .iter()
        .any(|claim| claim.status == project_store::NotificationReplyClaimStatus::Accepted));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));
}

#[test]
fn submit_notification_reply_cross_channel_race_accepts_single_winner_and_preserves_resume_truth() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_gate_linked_workflow_with_auto_continuation(&repo_root, &project_id, "approve_execution");

    project_store::upsert_notification_route(
        &repo_root,
        &project_store::NotificationRouteUpsertRecord {
            project_id: project_id.clone(),
            route_id: "route-telegram".into(),
            route_kind: "telegram".into(),
            route_target: "telegram:ops-room".into(),
            enabled: true,
            metadata_json: Some("{\"label\":\"ops\"}".into()),
            updated_at: "2026-04-16T14:59:58Z".into(),
        },
    )
    .expect("upsert telegram route");
    upsert_notification_route(&repo_root, &project_id, "route-discord");

    let approval = project_store::upsert_pending_operator_approval(
        &repo_root,
        &project_id,
        "session-1",
        Some("flow-1"),
        "approve_execution",
        "Approve execution",
        "Operator approval required.",
        "2026-04-16T20:50:00Z",
    )
    .expect("persist pending approval");

    let dispatches =
        load_notification_dispatches_for_action(&repo_root, &project_id, &approval.action_id);
    assert_eq!(dispatches.len(), 2);

    let telegram_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-telegram")
        .expect("telegram dispatch row");
    let discord_dispatch = dispatches
        .iter()
        .find(|dispatch| dispatch.route_id == "route-discord")
        .expect("discord dispatch row");

    let first = submit_notification_reply(
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: approval.action_id.clone(),
            route_id: telegram_dispatch.route_id.clone(),
            correlation_key: telegram_dispatch.correlation_key.clone(),
            responder_id: Some("telegram-operator".into()),
            reply_text: "Approve from telegram".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:50:05Z".into(),
        },
    )
    .expect("first channel reply should claim and resume");

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
        app.handle().clone(),
        SubmitNotificationReplyRequestDto {
            project_id: project_id.clone(),
            action_id: approval.action_id.clone(),
            route_id: discord_dispatch.route_id.clone(),
            correlation_key: discord_dispatch.correlation_key.clone(),
            responder_id: Some("discord-operator".into()),
            reply_text: "Duplicate from discord".into(),
            decision: "approve".into(),
            received_at: "2026-04-16T20:50:06Z".into(),
        },
    )
    .expect_err("late cross-channel reply should be rejected");
    assert_eq!(duplicate.code, "notification_reply_already_claimed");

    let claims = project_store::load_notification_reply_claims(
        &repo_root,
        &project_id,
        Some(&approval.action_id),
    )
    .expect("load cross-channel reply claims");
    assert_eq!(claims.len(), 2);
    assert!(claims
        .iter()
        .any(|claim| { claim.status == project_store::NotificationReplyClaimStatus::Accepted }));
    assert!(claims.iter().any(|claim| {
        claim.status == project_store::NotificationReplyClaimStatus::Rejected
            && claim.rejection_code.as_deref() == Some("notification_reply_already_claimed")
    }));

    let snapshot = project_store::load_project_snapshot(&repo_root, &project_id)
        .expect("load project snapshot")
        .snapshot;
    let approval_after = snapshot
        .approval_requests
        .iter()
        .find(|pending| pending.action_id == approval.action_id)
        .expect("approval after reply race");
    assert_eq!(approval_after.status, OperatorApprovalStatus::Approved);
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].source_action_id.as_deref(),
        Some(approval.action_id.as_str())
    );
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );
}

#[test]
fn planning_lifecycle_completion_branch_auto_dispatches_to_roadmap_without_duplicate_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_planning_lifecycle_workflow(&repo_root, &project_id, false);

    let discussion_to_research = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: project_id.clone(),
            transition_id: "lifecycle-discussion-research-1".into(),
            causal_transition_id: None,
            from_node_id: "discussion".into(),
            to_node_id: "research".into(),
            transition_kind: "advance".into(),
            gate_decision: "approved".into(),
            gate_decision_context: Some("discussion complete".into()),
            gate_updates: Vec::new(),
            occurred_at: "2026-04-16T16:00:00Z".into(),
        },
    )
    .expect("discussion -> research transition should persist and auto-dispatch to requirements");

    assert_eq!(
        discussion_to_research.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Applied
    );

    let research_to_requirements = discussion_to_research
        .automatic_dispatch
        .transition_event
        .clone()
        .expect("discussion -> research should auto-dispatch research -> requirements");
    assert_eq!(research_to_requirements.from_node_id, "research");
    assert_eq!(research_to_requirements.to_node_id, "requirements");
    assert_eq!(
        research_to_requirements.causal_transition_id.as_deref(),
        Some("lifecycle-discussion-research-1")
    );

    let research_handoff = discussion_to_research
        .automatic_dispatch
        .handoff_package
        .clone()
        .and_then(|outcome| outcome.package)
        .expect("research -> requirements auto-dispatch should persist a handoff package");
    assert_eq!(
        research_handoff.handoff_transition_id,
        research_to_requirements.transition_id
    );
    assert_eq!(
        research_handoff.causal_transition_id.as_deref(),
        Some("lifecycle-discussion-research-1")
    );

    let requirements_trigger = replay_transition_request(&project_id, &research_to_requirements);

    let requirements_to_roadmap = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        requirements_trigger.clone(),
    )
    .expect("replayed requirements trigger should auto-dispatch into roadmap");

    assert_eq!(
        requirements_to_roadmap.transition_event.transition_id,
        research_to_requirements.transition_id
    );
    assert_eq!(
        requirements_to_roadmap.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Applied
    );

    let roadmap_transition = requirements_to_roadmap
        .automatic_dispatch
        .transition_event
        .clone()
        .expect("requirements replay should auto-dispatch requirements -> roadmap");
    assert_eq!(roadmap_transition.from_node_id, "requirements");
    assert_eq!(roadmap_transition.to_node_id, "roadmap");
    assert_eq!(
        roadmap_transition.causal_transition_id.as_deref(),
        Some(research_to_requirements.transition_id.as_str())
    );

    let roadmap_handoff = requirements_to_roadmap
        .automatic_dispatch
        .handoff_package
        .clone()
        .and_then(|outcome| outcome.package)
        .expect("requirements -> roadmap auto-dispatch should persist a handoff package");
    assert_eq!(
        roadmap_handoff.handoff_transition_id,
        roadmap_transition.transition_id
    );
    assert_eq!(
        roadmap_handoff.causal_transition_id.as_deref(),
        Some(research_to_requirements.transition_id.as_str())
    );

    let replayed_requirements_trigger = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        requirements_trigger,
    )
    .expect("replaying the same requirements trigger should remain idempotent");

    assert_eq!(
        replayed_requirements_trigger.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Replayed
    );
    let replayed_roadmap_transition = replayed_requirements_trigger
        .automatic_dispatch
        .transition_event
        .clone()
        .expect("replayed requirements trigger should return persisted roadmap transition");
    assert_eq!(
        replayed_roadmap_transition.transition_id,
        roadmap_transition.transition_id
    );

    let replayed_roadmap_handoff = replayed_requirements_trigger
        .automatic_dispatch
        .handoff_package
        .clone()
        .and_then(|outcome| outcome.package)
        .expect("replayed requirements trigger should replay persisted roadmap handoff package");
    assert_eq!(
        replayed_roadmap_handoff.package_hash,
        roadmap_handoff.package_hash
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 2);
    assert_eq!(
        count_pending_gate_approval_rows(&repo_root, &project_id, "roadmap_gate"),
        0
    );

    let persisted_roadmap_handoff = project_store::load_workflow_handoff_package(
        &repo_root,
        &project_id,
        &roadmap_transition.transition_id,
    )
    .expect("load persisted roadmap handoff package")
    .expect("roadmap transition should have a persisted handoff package");
    assert_eq!(
        persisted_roadmap_handoff.package_hash,
        roadmap_handoff.package_hash
    );

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after lifecycle completion branch");

    assert!(
        snapshot
            .approval_requests
            .iter()
            .all(|approval| approval.status != OperatorApprovalStatus::Pending),
        "completion branch should not leave pending operator approvals"
    );

    let roadmap_stage = snapshot
        .lifecycle
        .stages
        .iter()
        .find(|stage| stage.node_id == "roadmap")
        .expect("roadmap lifecycle stage should exist");
    assert_eq!(roadmap_stage.status, PhaseStatus::Active);
    assert!(
        !roadmap_stage.action_required,
        "roadmap stage should be actionable-free in completion branch"
    );
}

#[test]
fn planning_lifecycle_gate_pause_branch_requires_explicit_resume_without_duplicate_rows() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_planning_lifecycle_workflow(&repo_root, &project_id, true);

    let discussion_to_research = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ApplyWorkflowTransitionRequestDto {
            project_id: project_id.clone(),
            transition_id: "lifecycle-discussion-research-pause-1".into(),
            causal_transition_id: None,
            from_node_id: "discussion".into(),
            to_node_id: "research".into(),
            transition_kind: "advance".into(),
            gate_decision: "approved".into(),
            gate_decision_context: Some("discussion complete".into()),
            gate_updates: Vec::new(),
            occurred_at: "2026-04-16T16:10:00Z".into(),
        },
    )
    .expect("discussion -> research transition should persist and auto-dispatch to requirements");

    let research_to_requirements = discussion_to_research
        .automatic_dispatch
        .transition_event
        .clone()
        .expect("discussion -> research should auto-dispatch research -> requirements");

    let requirements_trigger = replay_transition_request(&project_id, &research_to_requirements);

    let paused = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        requirements_trigger.clone(),
    )
    .expect("requirements replay should pause at unresolved roadmap gate");

    assert_eq!(
        paused.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        paused.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    assert!(paused.automatic_dispatch.transition_event.is_none());
    assert!(paused.automatic_dispatch.handoff_package.is_none());

    let pause_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after lifecycle gate pause");

    let pending_roadmap_approvals: Vec<_> = pause_snapshot
        .approval_requests
        .iter()
        .filter(|approval| {
            approval.gate_key.as_deref() == Some("roadmap_gate")
                && approval.status == OperatorApprovalStatus::Pending
        })
        .collect();
    assert_eq!(pending_roadmap_approvals.len(), 1);

    let pending_roadmap_approval = pending_roadmap_approvals[0];
    assert_eq!(
        pending_roadmap_approval.gate_node_id.as_deref(),
        Some("roadmap")
    );
    assert_eq!(
        pending_roadmap_approval.transition_from_node_id.as_deref(),
        Some("requirements")
    );
    assert_eq!(
        pending_roadmap_approval.transition_to_node_id.as_deref(),
        Some("roadmap")
    );
    assert_eq!(
        pending_roadmap_approval.transition_kind.as_deref(),
        Some("advance")
    );
    assert_eq!(pending_roadmap_approval.action_type, "approve_roadmap");

    let pending_action_id = pending_roadmap_approval.action_id.clone();
    let paused_message = paused
        .automatic_dispatch
        .message
        .as_deref()
        .expect("gate pause diagnostics should include a message");
    assert!(
        paused_message.contains(pending_action_id.as_str()),
        "expected gate-pause diagnostics to include deterministic pending action id, got {paused_message}"
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 2);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 1);
    assert_eq!(
        count_pending_gate_approval_rows(&repo_root, &project_id, "roadmap_gate"),
        1
    );
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &pending_action_id),
        1
    );

    let replayed_pause = apply_workflow_transition(
        app.handle().clone(),
        app.state::<DesktopState>(),
        requirements_trigger,
    )
    .expect("replayed requirements trigger should keep gate pause idempotent");

    assert_eq!(
        replayed_pause.automatic_dispatch.status,
        WorkflowAutomaticDispatchStatusDto::Skipped
    );
    assert_eq!(
        replayed_pause.automatic_dispatch.code.as_deref(),
        Some("workflow_transition_gate_unmet")
    );
    let replayed_pause_message = replayed_pause
        .automatic_dispatch
        .message
        .as_deref()
        .expect("replayed gate pause diagnostics should include message");
    assert!(
        replayed_pause_message.contains(pending_action_id.as_str()),
        "expected replayed gate-pause diagnostics to keep deterministic action id, got {replayed_pause_message}"
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 2);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 1);
    assert_eq!(
        count_pending_gate_approval_rows(&repo_root, &project_id, "roadmap_gate"),
        1
    );
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &pending_action_id),
        1
    );

    let missing_approval_error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("resume must fail while gate-linked approval is still unresolved");
    assert_eq!(
        missing_approval_error.code,
        "operator_resume_requires_approved_action"
    );

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("Roadmap gate approved by operator.".into()),
        },
    )
    .expect("resolve pending roadmap gate approval");

    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: pending_action_id.clone(),
            user_answer: None,
        },
    )
    .expect("resume should continue requirements -> roadmap once gate approval is provided");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);

    let events =
        project_store::load_recent_workflow_transition_events(&repo_root, &project_id, None)
            .expect("load transition events after gate pause resume");
    assert_eq!(events.len(), 3);

    let resumed_transition = events
        .iter()
        .find(|event| event.from_node_id == "requirements" && event.to_node_id == "roadmap")
        .expect("resumed transition should persist requirements -> roadmap event");
    assert!(
        resumed_transition.transition_id.starts_with("resume:"),
        "expected deterministic resume transition id, got {}",
        resumed_transition.transition_id
    );
    assert_eq!(
        resumed_transition.causal_transition_id.as_deref(),
        Some(research_to_requirements.transition_id.as_str())
    );

    assert_eq!(count_workflow_transition_rows(&repo_root, &project_id), 3);
    assert_eq!(count_workflow_handoff_rows(&repo_root, &project_id), 1);
    assert_eq!(
        count_pending_gate_approval_rows(&repo_root, &project_id, "roadmap_gate"),
        0
    );
    assert_eq!(
        count_operator_approval_rows_for_action(&repo_root, &project_id, &pending_action_id),
        1
    );

    let resumed_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto { project_id },
    )
    .expect("load project snapshot after gate pause resume");
    assert!(
        resumed_snapshot.approval_requests.iter().all(|approval| {
            !(approval.gate_key.as_deref() == Some("roadmap_gate")
                && approval.status == OperatorApprovalStatus::Pending)
        }),
        "roadmap gate pause should clear after explicit resolve + resume"
    );
}

#[test]
fn resume_operator_run_delivers_approved_terminal_input_without_auth_event_drift() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-resume-success",
        "session-1",
        Some("flow-1"),
        &runtime_shell::script_prompt_read_echo_and_sleep(
            "Enter value: ",
            "value",
            "value=",
            5,
        ),
    );

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(&project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let replayed_count = match attached {
        SupervisorControlResponse::Attached { replayed_count, .. } => replayed_count,
        other => panic!("expected attach ack, got {other:?}"),
    };
    let frames = read_event_frames(&mut reader, replayed_count);
    let action_id = frames
        .iter()
        .find_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item:
                    SupervisorLiveEventPayload::ActionRequired {
                        action_id,
                        action_type,
                        ..
                    },
                ..
            } => {
                assert_eq!(action_type, "terminal_input_required");
                Some(action_id.clone())
            }
            _ => None,
        })
        .expect("expected action-required replay frame");

    let missing_answer = resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: None,
        },
    )
    .expect_err("runtime-scoped approvals should fail closed when answer is missing");
    assert_eq!(missing_answer.code, "operator_action_answer_required");

    let pending_snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after missing-answer resolve attempt");
    let pending_approval = pending_snapshot
        .approval_requests
        .iter()
        .find(|approval| approval.action_id == action_id)
        .expect("runtime approval should remain pending after missing-answer failure");
    assert_eq!(pending_approval.status, OperatorApprovalStatus::Pending);
    assert!(pending_snapshot.verification_records.is_empty());
    assert!(pending_snapshot.resume_history.is_empty());

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve interactive operator action");

    recorder.clear();
    let resumed = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id,
            user_answer: None,
        },
    )
    .expect("resume runtime run with approved terminal input");

    assert_eq!(resumed.resume_entry.status, ResumeHistoryStatus::Started);
    assert_eq!(recorder.runtime_update_count(), 0);
    assert!(recorder.runtime_run_update_count() >= 1);

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut saw_delivery = false;
    let mut saw_transcript = false;
    while Instant::now() < deadline && !(saw_delivery && saw_transcript) {
        match read_supervisor_response(&mut reader) {
            Some(SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Activity { code, .. },
                ..
            }) if code == "runtime_supervisor_input_delivered" => saw_delivery = true,
            Some(SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Transcript { text },
                ..
            }) if text == "value=approved" => saw_transcript = true,
            Some(_) => {}
            None => thread::sleep(Duration::from_millis(25)),
        }
    }

    assert!(saw_delivery, "expected input-delivered activity frame");
    assert!(saw_transcript, "expected resumed transcript output");

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after runtime resume");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Started
    );

    let running = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run after resume")
    .expect("runtime run should still exist");
    assert_eq!(running.status, RuntimeRunStatusDto::Running);
    assert_eq!(
        running.transport.liveness,
        RuntimeRunTransportLivenessDto::Reachable
    );
    assert!(running.last_error_code.is_none());

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop resumed runtime run")
    .expect("runtime run should exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

#[test]
fn resume_operator_run_records_failed_history_when_runtime_identity_session_is_stale() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    let launched = launch_scripted_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        "run-resume-session-mismatch",
        "session-1",
        Some("flow-1"),
        &runtime_shell::script_prompt_read_echo_and_sleep(
            "Enter value: ",
            "value",
            "value=",
            5,
        ),
    );

    wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
            && runtime_run
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.kind == RuntimeRunCheckpointKindDto::ActionRequired)
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(&project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let replayed_count = match attached {
        SupervisorControlResponse::Attached { replayed_count, .. } => replayed_count,
        other => panic!("expected attach ack, got {other:?}"),
    };
    let frames = read_event_frames(&mut reader, replayed_count);
    let action_id = frames
        .iter()
        .find_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::ActionRequired { action_id, .. },
                ..
            } => Some(action_id.clone()),
            _ => None,
        })
        .expect("expected action-required replay frame");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve interactive operator action");

    let database_path = database_path_for_repo(&repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .execute(
            "UPDATE operator_approvals SET session_id = 'session-stale' WHERE project_id = ?1 AND action_id = ?2",
            [project_id.as_str(), action_id.as_str()],
        )
        .expect("corrupt runtime approval session identity");

    recorder.clear();
    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: action_id.clone(),
            user_answer: None,
        },
    )
    .expect_err("resume should fail when approved runtime session identity is stale");
    assert_eq!(error.code, "runtime_supervisor_session_mismatch");
    assert_eq!(recorder.runtime_update_count(), 0);
    assert!(recorder.runtime_run_update_count() >= 1);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after stale-session resume failure");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Failed
    );
    assert_eq!(
        snapshot.resume_history[0].source_action_id.as_deref(),
        Some(action_id.as_str())
    );

    let durable_runtime_run = project_store::load_runtime_run(&repo_root, &project_id)
        .expect("load durable runtime run after stale-session resume failure")
        .expect("durable runtime run should still exist after stale-session resume failure");
    assert_eq!(
        durable_runtime_run
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_session_mismatch")
    );

    let runtime_run = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run after stale-session resume failure")
    .expect("runtime run should still exist after stale-session resume failure");
    assert_eq!(runtime_run.status, RuntimeRunStatusDto::Running);
    assert_eq!(
        runtime_run.transport.liveness,
        RuntimeRunTransportLivenessDto::Reachable
    );

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run.run_id,
        },
    )
    .expect("stop runtime run after stale-session resume failure")
    .expect("runtime run should exist after stop");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

#[test]
fn resume_operator_run_records_failed_history_when_detached_control_channel_is_unreachable() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let recorder = attach_event_recorders(&app);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_unreachable_runtime_run(&repo_root, &project_id, "run-submit-failed");
    let persisted = project_store::upsert_runtime_action_required(
        &repo_root,
        &project_store::RuntimeActionRequiredUpsertRecord {
            project_id: project_id.clone(),
            run_id: "run-submit-failed".into(),
            runtime_kind: "openai_codex".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            transport_endpoint: "127.0.0.1:9".into(),
            started_at: "2026-04-15T19:00:00Z".into(),
            last_heartbeat_at: Some("2026-04-15T19:00:10Z".into()),
            last_error: None,
            boundary_id: "boundary-1".into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
            checkpoint_summary: "Detached runtime blocked on terminal input and is awaiting operator approval.".into(),
            created_at: "2026-04-15T19:00:12Z".into(),
        },
    )
    .expect("persist runtime action-required approval");

    resolve_operator_action(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResolveOperatorActionRequestDto {
            project_id: project_id.clone(),
            action_id: persisted.approval_request.action_id.clone(),
            decision: "approve".into(),
            user_answer: Some("approved".into()),
        },
    )
    .expect("approve unreachable runtime action");

    recorder.clear();
    let error = resume_operator_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ResumeOperatorRunRequestDto {
            project_id: project_id.clone(),
            action_id: persisted.approval_request.action_id,
            user_answer: None,
        },
    )
    .expect_err("resume should fail when detached control channel is unreachable");
    assert_eq!(error.code, "runtime_supervisor_connect_failed");
    assert_eq!(recorder.runtime_update_count(), 0);
    assert!(recorder.runtime_run_update_count() >= 1);

    let snapshot = get_project_snapshot(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("load project snapshot after failed runtime resume");
    assert_eq!(snapshot.resume_history.len(), 1);
    assert_eq!(
        snapshot.resume_history[0].status,
        ResumeHistoryStatus::Failed
    );
    assert_eq!(
        snapshot.resume_history[0].source_action_id.as_deref(),
        Some(snapshot.approval_requests[0].action_id.as_str())
    );

    let durable_runtime_run = project_store::load_runtime_run(&repo_root, &project_id)
        .expect("load durable runtime run after failed resume")
        .expect("durable runtime run should still exist after failed resume");
    assert_eq!(
        durable_runtime_run
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_connect_failed")
    );

    let runtime_run = get_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        GetRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("get runtime run after failed resume")
    .expect("runtime run should still exist after failed resume");
    assert_eq!(runtime_run.status, RuntimeRunStatusDto::Stale);
    assert_eq!(
        runtime_run.transport.liveness,
        RuntimeRunTransportLivenessDto::Unreachable
    );
}

#[test]
fn start_runtime_run_replaces_stale_row_with_new_reachable_run() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    seed_unreachable_runtime_run(&repo_root, &project_id, "run-stale");

    let launched = start_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StartRuntimeRunRequestDto {
            project_id: project_id.clone(),
        },
    )
    .expect("start runtime run after stale row");
    assert_ne!(launched.run_id, "run-stale");

    let running = wait_for_runtime_run(&app, &project_id, |runtime_run| {
        runtime_run.status == RuntimeRunStatusDto::Running
            && runtime_run.transport.liveness == RuntimeRunTransportLivenessDto::Reachable
    });
    assert_eq!(running.run_id, launched.run_id);

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: launched.run_id,
        },
    )
    .expect("stop replacement runtime run")
    .expect("replacement runtime run should exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stopped);
}

#[test]
fn stop_runtime_run_rejects_mismatched_run_id_and_marks_unreachable_sidecar_stale() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_unreachable_runtime_run(&repo_root, &project_id, "run-1");

    let mismatch = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id: project_id.clone(),
            run_id: "run-2".into(),
        },
    )
    .expect_err("mismatched run id should fail closed");
    assert_eq!(mismatch.code, "runtime_run_mismatch");

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: "run-1".into(),
        },
    )
    .expect("stop against unreachable sidecar should return durable snapshot")
    .expect("durable runtime run should still exist");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Stale);
    assert_eq!(
        stopped.transport.liveness,
        RuntimeRunTransportLivenessDto::Unreachable
    );
    assert_eq!(
        stopped.last_error_code.as_deref(),
        Some("supervisor_stop_failed")
    );
}

#[test]
fn stop_runtime_run_returns_existing_terminal_snapshot_after_sidecar_exit() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);

    seed_failed_runtime_run(&repo_root, &project_id, "run-failed");

    let stopped = stop_runtime_run(
        app.handle().clone(),
        app.state::<DesktopState>(),
        StopRuntimeRunRequestDto {
            project_id,
            run_id: "run-failed".into(),
        },
    )
    .expect("stop after sidecar exit should succeed")
    .expect("terminal runtime run should still be returned");
    assert_eq!(stopped.status, RuntimeRunStatusDto::Failed);
    assert_eq!(
        stopped.last_error_code.as_deref(),
        Some("runtime_supervisor_exit_nonzero")
    );
}

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}
