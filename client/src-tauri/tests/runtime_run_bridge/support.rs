pub(crate) use std::{
    io::{BufRead, BufReader, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, OnceLock},
    thread,
    time::{Duration, Instant},
};

pub(crate) use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
pub(crate) use cadence_desktop_lib::{
    auth::{
        anthropic::AnthropicAuthConfig, persist_openai_codex_session, sync_openai_profile_link,
        StoredOpenAiCodexSession,
    },
    commands::{
        apply_workflow_transition::apply_workflow_transition,
        cancel_autonomous_run::cancel_autonomous_run, get_autonomous_run::get_autonomous_run,
        get_project_snapshot::get_project_snapshot, get_runtime_run::get_runtime_run,
        get_runtime_session::get_runtime_session,
        provider_profiles::upsert_provider_profile,
        resolve_operator_action::resolve_operator_action,
        resume_operator_run::resume_operator_run, start_autonomous_run::start_autonomous_run,
        start_runtime_run::start_runtime_run, start_runtime_session::start_runtime_session,
        stop_runtime_run::stop_runtime_run, submit_notification_reply::submit_notification_reply,
        update_runtime_run_controls::update_runtime_run_controls,
        ApplyWorkflowTransitionRequestDto, AutonomousRunRecoveryStateDto, AutonomousRunStateDto,
        AutonomousRunStatusDto, AutonomousUnitStatusDto, CancelAutonomousRunRequestDto,
        GetAutonomousRunRequestDto, GetRuntimeRunRequestDto, NotificationDispatchStatusDto,
        NotificationReplyClaimStatusDto, OperatorApprovalStatus, PhaseStatus, PhaseStep,
        ProjectIdRequestDto, ProjectUpdateReason, ProjectUpdatedPayloadDto,
        ResolveOperatorActionRequestDto, ResumeHistoryStatus, ResumeOperatorRunRequestDto,
        RuntimeAuthPhase, RuntimeRunApprovalModeDto, RuntimeRunCheckpointKindDto,
        RuntimeRunControlInputDto, RuntimeRunDto, RuntimeRunStatusDto,
        RuntimeRunTransportLivenessDto, RuntimeRunUpdatedPayloadDto, RuntimeUpdatedPayloadDto,
        StartAutonomousRunRequestDto, StartRuntimeRunRequestDto, StopRuntimeRunRequestDto,
        SubmitNotificationReplyRequestDto, UpdateRuntimeRunControlsRequestDto,
        UpsertProviderProfileRequestDto, WorkflowAutomaticDispatchStatusDto,
        PROJECT_UPDATED_EVENT, RUNTIME_RUN_UPDATED_EVENT, RUNTIME_UPDATED_EVENT,
    },
    configure_builder_with_state,
    db::{self, database_path_for_repo, project_store},
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    runtime::{
        autonomous_orchestrator::persist_supervisor_event,
        launch_detached_runtime_supervisor,
        protocol::{
            SupervisorControlRequest, SupervisorControlResponse, SupervisorLiveEventPayload,
        },
        RuntimeSupervisorLaunchRequest,
    },
    state::DesktopState,
};
pub(crate) use serde_json::json;
pub(crate) use tauri::{Listener, Manager};
pub(crate) use tempfile::TempDir;

#[path = "../support/runtime_shell.rs"]
pub(crate) mod runtime_shell;

#[path = "../support/supervisor_test_lock.rs"]
pub(crate) mod supervisor_test_lock;

pub(crate) struct SupervisorTestGuard {
    _in_process: MutexGuard<'static, ()>,
    _cross_process: supervisor_test_lock::SupervisorProcessLock,
}

impl Drop for SupervisorTestGuard {
    fn drop(&mut self) {
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

#[derive(Clone, Default)]
pub(crate) struct EventRecorder {
    project_updates: Arc<Mutex<Vec<ProjectUpdatedPayloadDto>>>,
    runtime_updates: Arc<Mutex<Vec<RuntimeUpdatedPayloadDto>>>,
    runtime_run_updates: Arc<Mutex<Vec<RuntimeRunUpdatedPayloadDto>>>,
}

impl EventRecorder {
    pub(crate) fn clear(&self) {
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

    pub(crate) fn project_update_count(&self) -> usize {
        self.project_updates
            .lock()
            .expect("project updates lock")
            .len()
    }

    pub(crate) fn latest_project_update(&self) -> Option<ProjectUpdatedPayloadDto> {
        self.project_updates
            .lock()
            .expect("project updates lock")
            .last()
            .cloned()
    }

    pub(crate) fn runtime_update_count(&self) -> usize {
        self.runtime_updates
            .lock()
            .expect("runtime updates lock")
            .len()
    }

    pub(crate) fn runtime_run_update_count(&self) -> usize {
        self.runtime_run_updates
            .lock()
            .expect("runtime run updates lock")
            .len()
    }
}

pub(crate) fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

pub(crate) fn create_state(root: &TempDir) -> (DesktopState, PathBuf, PathBuf) {
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
    (
        DesktopState::default()
            .with_registry_file_override(registry_path.clone())
            .with_auth_store_file_override(auth_store_path.clone())
            .with_provider_profiles_file_override(provider_profiles_path)
            .with_provider_profile_credential_store_file_override(provider_profile_credentials_path)
            .with_runtime_settings_file_override(runtime_settings_path)
            .with_openrouter_credential_file_override(openrouter_credential_path)
            .with_runtime_supervisor_binary_override(supervisor_binary_path()),
        registry_path,
        auth_store_path,
    )
}

pub(crate) fn anthropic_auth_config(models_url: String) -> AnthropicAuthConfig {
    AnthropicAuthConfig {
        models_url,
        timeout: Duration::from_secs(5),
        ..AnthropicAuthConfig::default()
    }
}

pub(crate) fn spawn_static_http_server(status: u16, body: &str) -> String {
    spawn_static_http_server_for_requests(status, body, 1)
}

pub(crate) fn spawn_static_http_server_for_requests(
    status: u16,
    body: &str,
    request_count: usize,
) -> String {
    assert!(request_count > 0, "request_count must be positive");

    let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).expect("bind test http server");
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
                "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body,
            )
            .expect("write test http response");
        }
    });

    format!("http://{address}")
}

pub(crate) fn seed_anthropic_profile(
    app: &tauri::App<tauri::test::MockRuntime>,
    profile_id: &str,
    model_id: &str,
    api_key: &str,
) {
    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: profile_id.into(),
            provider_id: "anthropic".into(),
            runtime_kind: "anthropic".into(),
            label: "Anthropic Work".into(),
            model_id: model_id.into(),
            preset_id: Some("anthropic".into()),
            base_url: None,
            api_version: None,
            api_key: Some(api_key.into()),
            activate: true,
        },
    )
    .expect("seed anthropic profile");
}

pub(crate) fn seed_openai_compatible_profile(
    app: &tauri::App<tauri::test::MockRuntime>,
    profile_id: &str,
    provider_id: &str,
    runtime_kind: &str,
    model_id: &str,
    preset_id: Option<&str>,
    base_url: Option<&str>,
    api_version: Option<&str>,
    api_key: &str,
) {
    upsert_provider_profile(
        app.handle().clone(),
        app.state::<DesktopState>(),
        UpsertProviderProfileRequestDto {
            profile_id: profile_id.into(),
            provider_id: provider_id.into(),
            runtime_kind: runtime_kind.into(),
            label: profile_id.into(),
            model_id: model_id.into(),
            preset_id: preset_id.map(str::to_string),
            base_url: base_url.map(str::to_string),
            api_version: api_version.map(str::to_string),
            api_key: Some(api_key.into()),
            activate: true,
        },
    )
    .expect("seed openai-compatible profile");
}

pub(crate) fn attach_event_recorders(app: &tauri::App<tauri::test::MockRuntime>) -> EventRecorder {
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

pub(crate) fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_Cadence-runtime-supervisor"))
}

pub(crate) fn jwt_with_account_id(account_id: &str) -> String {
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

pub(crate) fn seed_authenticated_runtime(
    app: &tauri::App<tauri::test::MockRuntime>,
    auth_store_path: &Path,
    project_id: &str,
) {
    let stored_session = StoredOpenAiCodexSession {
        provider_id: "openai_codex".into(),
        session_id: "session-auth".into(),
        account_id: "acct-1".into(),
        access_token: jwt_with_account_id("acct-1"),
        refresh_token: "refresh-1".into(),
        expires_at: current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
        updated_at: "2026-04-13T14:11:59Z".into(),
    };

    persist_openai_codex_session(auth_store_path, stored_session.clone())
        .expect("persist auth session");
    sync_openai_profile_link(
        &app.handle().clone(),
        &app.state::<DesktopState>(),
        Some("openai_codex-default"),
        Some(&stored_session),
    )
    .expect("sync active provider-profile auth link");

    let runtime = start_runtime_session(
        app.handle().clone(),
        app.state::<DesktopState>(),
        ProjectIdRequestDto {
            project_id: project_id.into(),
        },
    )
    .expect("start runtime session");
    assert_eq!(
        runtime.phase,
        RuntimeAuthPhase::Authenticated,
        "expected authenticated runtime session after seeding auth; last_error_code={:?}, last_error={:?}",
        runtime.last_error_code,
        runtime.last_error,
    );
}

pub(crate) fn seed_gate_linked_workflow_with_auto_continuation(
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

pub(crate) fn seed_gate_linked_workflow_with_blocked_auto_continuation(
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

pub(crate) fn seed_planning_lifecycle_workflow(
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

pub(crate) fn replay_transition_request(
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

pub(crate) fn wait_for_runtime_run(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    predicate: impl Fn(&RuntimeRunDto) -> bool,
) -> RuntimeRunDto {
    let deadline = Instant::now() + Duration::from_secs(12);

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

pub(crate) fn wait_for_autonomous_run(
    app: &tauri::App<tauri::test::MockRuntime>,
    project_id: &str,
    predicate: impl Fn(&AutonomousRunStateDto) -> bool,
) -> AutonomousRunStateDto {
    let deadline = Instant::now() + Duration::from_secs(12);

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

pub(crate) fn count_runtime_run_rows(repo_root: &Path) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row("SELECT COUNT(*) FROM runtime_runs", [], |row| row.get(0))
        .expect("count runtime runs")
}

pub(crate) fn count_autonomous_run_rows(repo_root: &Path) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row("SELECT COUNT(*) FROM autonomous_runs", [], |row| row.get(0))
        .expect("count autonomous runs")
}

pub(crate) fn count_autonomous_unit_rows(repo_root: &Path, project_id: &str, run_id: &str) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM autonomous_units WHERE project_id = ?1 AND run_id = ?2",
            [project_id, run_id],
            |row| row.get(0),
        )
        .expect("count autonomous unit rows")
}

pub(crate) fn count_autonomous_attempt_rows(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
) -> i64 {
    let database_path = database_path_for_repo(repo_root);
    let connection = rusqlite::Connection::open(&database_path).expect("open runtime db");
    connection
        .query_row(
            "SELECT COUNT(*) FROM autonomous_unit_attempts WHERE project_id = ?1 AND run_id = ?2",
            [project_id, run_id],
            |row| row.get(0),
        )
        .expect("count autonomous unit attempt rows")
}

pub(crate) fn count_workflow_transition_rows(repo_root: &Path, project_id: &str) -> i64 {
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

pub(crate) fn count_workflow_handoff_rows(repo_root: &Path, project_id: &str) -> i64 {
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

pub(crate) fn count_pending_gate_approval_rows(
    repo_root: &Path,
    project_id: &str,
    gate_key: &str,
) -> i64 {
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

pub(crate) fn count_operator_approval_rows_for_action(
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

pub(crate) fn upsert_notification_route(repo_root: &Path, project_id: &str, route_id: &str) {
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

pub(crate) fn load_notification_dispatches_for_action(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
) -> Vec<project_store::NotificationDispatchRecord> {
    project_store::load_notification_dispatches(repo_root, project_id, Some(action_id))
        .expect("load notification dispatches for action")
}

pub(crate) fn wait_for_notification_dispatches_for_action(
    repo_root: &Path,
    project_id: &str,
    action_id: &str,
    expected_count: usize,
) -> Vec<project_store::NotificationDispatchRecord> {
    let deadline = Instant::now() + Duration::from_secs(3);

    loop {
        let dispatches = load_notification_dispatches_for_action(repo_root, project_id, action_id);
        if dispatches.len() == expected_count {
            return dispatches;
        }

        let all_dispatches =
            project_store::load_notification_dispatches(repo_root, project_id, None)
                .expect("load all notification dispatches while waiting for action dispatches");
        let approval_count =
            count_operator_approval_rows_for_action(repo_root, project_id, action_id);
        assert!(
            Instant::now() < deadline,
            "timed out waiting for {expected_count} notification dispatch row(s) for action `{action_id}`, approval rows: {approval_count}, action rows: {dispatches:?}, all dispatches: {all_dispatches:?}"
        );
        thread::sleep(Duration::from_millis(50));
    }
}

pub(crate) fn seed_unreachable_runtime_run(repo_root: &Path, project_id: &str, run_id: &str) {
    project_store::upsert_runtime_run(
        repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
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
            control_state: Some(
                project_store::build_runtime_run_control_state(
                    "openai_codex",
                    None,
                    cadence_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest,
                    "2026-04-15T19:00:00Z",
                    None,
                )
                .expect("seed unreachable runtime run control state"),
            ),
        },
    )
    .expect("seed unreachable runtime run");
}

pub(crate) fn seed_failed_runtime_run(repo_root: &Path, project_id: &str, run_id: &str) {
    project_store::upsert_runtime_run(
        repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
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
            control_state: Some(
                project_store::build_runtime_run_control_state(
                    "openai_codex",
                    None,
                    cadence_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest,
                    "2026-04-15T19:00:00Z",
                    None,
                )
                .expect("seed failed runtime run control state"),
            ),
        },
    )
    .expect("seed failed runtime run");
}

pub(crate) fn seed_active_autonomous_run(repo_root: &Path, project_id: &str, run_id: &str) {
    let timestamp = "2026-04-16T12:00:00Z";
    let payload = project_store::AutonomousRunUpsertRecord {
        run: project_store::AutonomousRunRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            runtime_kind: "openai_codex".into(),
            provider_id: "openai_codex".into(),
            supervisor_kind: "detached_pty".into(),
            status: project_store::AutonomousRunStatus::Running,
            active_unit_sequence: Some(1),
            duplicate_start_detected: false,
            duplicate_start_run_id: None,
            duplicate_start_reason: None,
            started_at: timestamp.into(),
            last_heartbeat_at: Some(timestamp.into()),
            last_checkpoint_at: Some(timestamp.into()),
            paused_at: None,
            cancelled_at: None,
            completed_at: None,
            crashed_at: None,
            stopped_at: None,
            pause_reason: None,
            cancel_reason: None,
            crash_reason: None,
            last_error: None,
            updated_at: timestamp.into(),
        },
        unit: Some(project_store::AutonomousUnitRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            unit_id: format!("{run_id}:unit:1"),
            sequence: 1,
            kind: project_store::AutonomousUnitKind::Researcher,
            status: project_store::AutonomousUnitStatus::Active,
            summary: "Researcher child session launched.".into(),
            boundary_id: None,
            workflow_linkage: None,
            started_at: timestamp.into(),
            finished_at: None,
            updated_at: timestamp.into(),
            last_error: None,
        }),
        attempt: Some(project_store::AutonomousUnitAttemptRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            unit_id: format!("{run_id}:unit:1"),
            attempt_id: format!("{run_id}:unit:1:attempt:1"),
            attempt_number: 1,
            child_session_id: "child-session-1".into(),
            status: project_store::AutonomousUnitStatus::Active,
            boundary_id: None,
            workflow_linkage: None,
            started_at: timestamp.into(),
            finished_at: None,
            updated_at: timestamp.into(),
            last_error: None,
        }),
        artifacts: Vec::new(),
    };

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        match project_store::upsert_autonomous_run(repo_root, &payload) {
            Ok(_) => return,
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Err(error) => panic!("seed active autonomous run: {error:?}"),
        }
    }
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
    let shell = runtime_shell::launch_script(script);
    let timestamp = cadence_desktop_lib::auth::now_timestamp();

    launch_detached_runtime_supervisor(
        state,
        RuntimeSupervisorLaunchRequest {
            project_id: project_id.into(),
            repo_root: repo_root.to_path_buf(),
            runtime_kind: "openai_codex".into(),
            run_id: run_id.into(),
            session_id: session_id.into(),
            flow_id: flow_id.map(str::to_string),
            launch_context: cadence_desktop_lib::runtime::RuntimeSupervisorLaunchContext {
                provider_id: "openai_codex".into(),
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
            run_controls: project_store::build_runtime_run_control_state(
                "openai_codex",
                None,
                cadence_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest,
                &timestamp,
                None,
            )
            .expect("build scripted runtime run controls"),
        },
    )
    .expect("launch scripted runtime supervisor")
}

pub(crate) fn attach_reader(
    endpoint: &str,
    request: SupervisorControlRequest,
) -> BufReader<TcpStream> {
    let mut stream = TcpStream::connect(endpoint).expect("connect attach reader");
    stream
        .set_read_timeout(Some(Duration::from_millis(150)))
        .expect("set read timeout");
    serde_json::to_writer(&mut stream, &request).expect("serialize attach request");
    stream.write_all(b"\n").expect("write attach newline");
    stream.flush().expect("flush attach request");
    BufReader::new(stream)
}

pub(crate) fn read_supervisor_response(
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

pub(crate) fn expect_attach_ack(
    response: Option<SupervisorControlResponse>,
) -> SupervisorControlResponse {
    match response {
        Some(response @ SupervisorControlResponse::Attached { .. }) => response,
        other => panic!("expected attach ack, got {other:?}"),
    }
}

pub(crate) fn read_event_frames(
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

pub(crate) fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}
