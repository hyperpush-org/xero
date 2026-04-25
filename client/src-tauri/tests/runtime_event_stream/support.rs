pub(crate) use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{
        mpsc::{sync_channel, Receiver},
        Mutex, MutexGuard, OnceLock,
    },
    thread,
    time::{Duration, Instant},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
pub(crate) use cadence_desktop_lib::{
    auth::{now_timestamp, persist_openai_codex_session, StoredOpenAiCodexSession},
    commands::{
        start_runtime_session::start_runtime_session, AutonomousSkillCacheStatusDto,
        AutonomousSkillLifecycleResultDto, AutonomousSkillLifecycleStageDto, CommandError,
        CommandErrorClass, ProjectIdRequestDto, ProjectSummaryDto, ProjectUpdateReason,
        ProjectUpdatedPayloadDto, RuntimeAuthPhase, RuntimeRunCheckpointKindDto,
        RuntimeRunTransportLivenessDto, RuntimeSessionDto, RuntimeStreamItemDto,
        RuntimeStreamItemKind, RuntimeToolCallState, SubscribeRuntimeStreamResponseDto,
        ToolResultSummaryDto, SUBSCRIBE_RUNTIME_STREAM_COMMAND,
    },
    configure_builder_with_state,
    db::{
        self,
        project_store::{
            self, RuntimeActionRequiredUpsertRecord, RuntimeRunCheckpointKind,
            RuntimeRunCheckpointRecord, RuntimeRunDiagnosticRecord, RuntimeRunRecord,
            RuntimeRunStatus, RuntimeRunTransportLiveness, RuntimeRunTransportRecord,
            RuntimeRunUpsertRecord,
        },
    },
    git::repository::CanonicalRepository,
    registry::{self, RegistryProjectRecord},
    runtime::protocol::{
        SupervisorControlRequest, SupervisorControlResponse, SupervisorLiveEventPayload,
        SUPERVISOR_PROTOCOL_VERSION,
    },
    runtime::{
        launch_detached_runtime_supervisor, probe_runtime_run, start_runtime_stream,
        stop_runtime_run, RuntimeStreamRequest, RuntimeSupervisorLaunchRequest,
        RuntimeSupervisorProbeRequest, RuntimeSupervisorStopRequest,
    },
    state::DesktopState,
};
pub(crate) use serde::Serialize;
pub(crate) use serde_json::{json, Value};
pub(crate) use tauri::Manager;
pub(crate) use tempfile::TempDir;

#[path = "../support/runtime_shell.rs"]
pub(crate) mod runtime_shell;

#[path = "../support/supervisor_test_lock.rs"]
pub(crate) mod supervisor_test_lock;

pub(crate) const STRUCTURED_EVENT_PREFIX: &str = "__Cadence_EVENT__ ";
pub(crate) const STREAM_TIMEOUT: Duration = Duration::from_secs(5);

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

fn runtime_control_state(timestamp: &str) -> project_store::RuntimeRunControlStateRecord {
    project_store::build_runtime_run_control_state(
        "openai_codex",
        None,
        cadence_desktop_lib::commands::RuntimeRunApprovalModeDto::Suggest,
        timestamp,
        None,
    )
    .expect("build runtime control state")
}

pub(crate) fn build_mock_app(state: DesktopState) -> tauri::App<tauri::test::MockRuntime> {
    configure_builder_with_state(tauri::test::mock_builder(), state)
        .build(tauri::generate_context!())
        .expect("failed to build mock Tauri app")
}

pub(crate) fn create_state(root: &TempDir) -> (DesktopState, PathBuf, PathBuf) {
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

pub(crate) fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_Cadence-runtime-supervisor"))
}

pub(crate) fn invoke_request(command: &str, payload: Value) -> tauri::webview::InvokeRequest {
    tauri::webview::InvokeRequest {
        cmd: command.into(),
        callback: tauri::ipc::CallbackFn(0),
        error: tauri::ipc::CallbackFn(1),
        url: "http://tauri.localhost".parse().expect("valid mock URL"),
        body: tauri::ipc::InvokeBody::Json(payload),
        headers: Default::default(),
        invoke_key: tauri::test::INVOKE_KEY.to_string(),
    }
}

pub(crate) fn channel_string() -> String {
    serde_json::to_value(tauri::ipc::Channel::<RuntimeStreamItemDto>::new(|_| Ok(())))
        .expect("channel should serialize")
        .as_str()
        .expect("channel should serialize to string")
        .to_string()
}

pub(crate) fn subscribe_request(
    project_id: &str,
    raw_channel: &str,
    item_kinds: &[&str],
) -> tauri::webview::InvokeRequest {
    invoke_request(
        SUBSCRIBE_RUNTIME_STREAM_COMMAND,
        json!({
            "request": {
                "projectId": project_id,
                "agentSessionId": "agent-session-main",
                "channel": raw_channel,
                "itemKinds": item_kinds,
            }
        }),
    )
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

pub(crate) fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

pub(crate) fn seed_authenticated_runtime(
    app: &tauri::App<tauri::test::MockRuntime>,
    auth_store_path: &Path,
    project_id: &str,
) -> RuntimeSessionDto {
    persist_openai_codex_session(
        auth_store_path,
        StoredOpenAiCodexSession {
            provider_id: "openai_codex".into(),
            session_id: "session-auth".into(),
            account_id: "acct-1".into(),
            access_token: jwt_with_account_id("acct-1"),
            refresh_token: "refresh-1".into(),
            expires_at: current_unix_timestamp() + Duration::from_secs(3600).as_secs() as i64,
            updated_at: "2026-04-15T23:10:00Z".into(),
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

pub(crate) fn launch_request(
    project_id: &str,
    repo_root: &Path,
    run_id: &str,
    command: &str,
) -> RuntimeSupervisorLaunchRequest {
    let shell = runtime_shell::launch_script(command);
    RuntimeSupervisorLaunchRequest {
        project_id: project_id.into(),
        agent_session_id: "agent-session-main".into(),
        repo_root: repo_root.to_path_buf(),
        runtime_kind: "openai_codex".into(),
        run_id: run_id.into(),
        session_id: "session-auth".into(),
        flow_id: None,
        launch_context: cadence_desktop_lib::runtime::RuntimeSupervisorLaunchContext {
            provider_id: "openai_codex".into(),
            session_id: "session-auth".into(),
            flow_id: None,
            model_id: "openai_codex".into(),
            thinking_effort: None,
        },
        launch_env: cadence_desktop_lib::runtime::RuntimeSupervisorLaunchEnv::default(),
        program: shell.program,
        args: shell.args,
        startup_timeout: Duration::from_secs(5),
        control_timeout: Duration::from_millis(750),
        supervisor_binary: Some(supervisor_binary_path()),
        run_controls: RuntimeSupervisorLaunchRequest::default().run_controls,
    }
}

pub(crate) fn probe_request(project_id: &str, repo_root: &Path) -> RuntimeSupervisorProbeRequest {
    RuntimeSupervisorProbeRequest {
        project_id: project_id.into(),
        agent_session_id: "agent-session-main".into(),
        repo_root: repo_root.to_path_buf(),
        control_timeout: Duration::from_millis(750),
    }
}

pub(crate) fn stop_request(project_id: &str, repo_root: &Path) -> RuntimeSupervisorStopRequest {
    RuntimeSupervisorStopRequest {
        project_id: project_id.into(),
        agent_session_id: "agent-session-main".into(),
        repo_root: repo_root.to_path_buf(),
        control_timeout: Duration::from_millis(750),
        shutdown_timeout: Duration::from_secs(4),
    }
}

pub(crate) fn launch_supervised_run(
    state: &DesktopState,
    project_id: &str,
    repo_root: &Path,
    run_id: &str,
    command: &str,
) -> project_store::RuntimeRunSnapshotRecord {
    launch_detached_runtime_supervisor(
        state,
        launch_request(project_id, repo_root, run_id, command),
    )
    .expect("launch detached runtime supervisor")
}

pub(crate) fn wait_for_runtime_run(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    predicate: impl Fn(&project_store::RuntimeRunSnapshotRecord) -> bool,
) -> project_store::RuntimeRunSnapshotRecord {
    let deadline = Instant::now() + STREAM_TIMEOUT;

    loop {
        let snapshot = probe_runtime_run(state, probe_request(project_id, repo_root))
            .expect("probe runtime run")
            .expect("runtime run should exist");
        if predicate(&snapshot) {
            return snapshot;
        }

        assert!(
            Instant::now() < deadline,
            "timed out waiting for runtime run predicate, last snapshot: {snapshot:?}"
        );
        thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn stop_supervisor_run(state: &DesktopState, project_id: &str, repo_root: &Path) {
    let stopped = stop_runtime_run(state, stop_request(project_id, repo_root))
        .expect("stop runtime run should succeed")
        .expect("runtime run should exist");
    assert_eq!(stopped.run.project_id, project_id);
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
        match receiver.recv_timeout(STREAM_TIMEOUT) {
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
                item.sequence,
            );
        }
        previous = Some(item.sequence);
    }
}

pub(crate) fn seed_pending_operator_approval(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    session_id: &str,
) -> String {
    seed_runtime_action_required(
        repo_root,
        project_id,
        run_id,
        session_id,
        "boundary-pending",
        "2026-04-15T23:10:04Z",
    )
}

pub(crate) fn seed_terminal_runtime_run(repo_root: &Path, project_id: &str, run_id: &str) {
    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: project_id.into(),
                agent_session_id: "agent-session-main".into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: RuntimeRunStatus::Failed,
                transport: RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: "launch-pending".into(),
                    liveness: RuntimeRunTransportLiveness::Unknown,
                },
                started_at: "2026-04-15T23:10:00Z".into(),
                last_heartbeat_at: None,
                stopped_at: Some("2026-04-15T23:10:05Z".into()),
                last_error: Some(RuntimeRunDiagnosticRecord {
                    code: "runtime_supervisor_exit_nonzero".into(),
                    message: "The detached runtime supervisor exited with status 17.".into(),
                }),
                updated_at: "2026-04-15T23:10:05Z".into(),
            },
            checkpoint: None,
            control_state: Some(runtime_control_state("2026-04-15T23:10:05Z")),
        },
    )
    .expect("seed failed runtime run");
}

pub(crate) fn seed_fake_runtime_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    endpoint: &str,
) {
    let timestamp = now_timestamp();
    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: project_id.into(),
                agent_session_id: "agent-session-main".into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: RuntimeRunStatus::Running,
                transport: RuntimeRunTransportRecord {
                    kind: "tcp".into(),
                    endpoint: endpoint.into(),
                    liveness: RuntimeRunTransportLiveness::Reachable,
                },
                started_at: timestamp.clone(),
                last_heartbeat_at: Some(timestamp.clone()),
                stopped_at: None,
                last_error: None,
                updated_at: timestamp.clone(),
            },
            checkpoint: Some(RuntimeRunCheckpointRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                sequence: 1,
                kind: RuntimeRunCheckpointKind::Bootstrap,
                summary: "Supervisor ready".into(),
                created_at: timestamp.clone(),
            }),
            control_state: Some(runtime_control_state(&timestamp)),
        },
    )
    .expect("seed fake runtime run");
}

pub(crate) fn seed_runtime_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    session_id: &str,
    boundary_id: &str,
    created_at: &str,
) -> String {
    let runtime_run = project_store::load_runtime_run(
        repo_root,
        project_id,
        project_store::DEFAULT_AGENT_SESSION_ID,
    )
    .expect("load runtime run for action-required seed")
    .expect("runtime run should exist before seeding action-required state");

    let persisted = project_store::upsert_runtime_action_required(
        repo_root,
        &RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
            agent_session_id: "agent-session-main".into(),
            run_id: run_id.into(),
            runtime_kind: runtime_run.run.runtime_kind,
            session_id: session_id.into(),
            flow_id: None,
            transport_endpoint: runtime_run.run.transport.endpoint,
            started_at: runtime_run.run.started_at,
            last_heartbeat_at: runtime_run
                .run
                .last_heartbeat_at
                .or_else(|| Some(created_at.into())),
            last_error: runtime_run.run.last_error,
            boundary_id: boundary_id.into(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Provide terminal input to continue this run.".into(),
            checkpoint_summary: "Action required checkpoint recorded".into(),
            created_at: created_at.into(),
        },
    )
    .expect("seed runtime action required");

    persisted.approval_request.action_id
}

pub(crate) fn seed_blocked_autonomous_run(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    action_id: &str,
    boundary_id: &str,
) {
    let timestamp = "2026-04-15T23:10:02Z";
    let unit_id = format!("{run_id}:unit:1");
    let attempt_id = format!("{run_id}:unit:1:attempt:1");
    let artifact_id = format!("{attempt_id}:boundary:{boundary_id}:blocked");

    project_store::upsert_autonomous_run(
        repo_root,
        &project_store::AutonomousRunUpsertRecord {
            run: project_store::AutonomousRunRecord {
                project_id: project_id.into(),
                agent_session_id: "agent-session-main".into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
                provider_id: "openai_codex".into(),
                supervisor_kind: "detached_pty".into(),
                status: project_store::AutonomousRunStatus::Paused,
                active_unit_sequence: Some(1),
                duplicate_start_detected: false,
                duplicate_start_run_id: None,
                duplicate_start_reason: None,
                started_at: timestamp.into(),
                last_heartbeat_at: Some(timestamp.into()),
                last_checkpoint_at: Some(timestamp.into()),
                paused_at: Some(timestamp.into()),
                cancelled_at: None,
                completed_at: None,
                crashed_at: None,
                stopped_at: None,
                pause_reason: Some(project_store::RuntimeRunDiagnosticRecord {
                    code: "autonomous_operator_action_required".into(),
                    message: "Provide terminal input to continue this run.".into(),
                }),
                cancel_reason: None,
                crash_reason: None,
                last_error: None,
                updated_at: timestamp.into(),
            },
            unit: Some(project_store::AutonomousUnitRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                unit_id: unit_id.clone(),
                sequence: 1,
                kind: project_store::AutonomousUnitKind::Researcher,
                status: project_store::AutonomousUnitStatus::Blocked,
                summary: "Blocked on operator boundary `Terminal input required`.".into(),
                boundary_id: Some(boundary_id.into()),
                workflow_linkage: None,
                started_at: timestamp.into(),
                finished_at: None,
                updated_at: timestamp.into(),
                last_error: None,
            }),
            attempt: Some(project_store::AutonomousUnitAttemptRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                unit_id: unit_id.clone(),
                attempt_id: attempt_id.clone(),
                attempt_number: 1,
                child_session_id: "child-session-1".into(),
                status: project_store::AutonomousUnitStatus::Blocked,
                boundary_id: Some(boundary_id.into()),
                workflow_linkage: None,
                started_at: timestamp.into(),
                finished_at: None,
                updated_at: timestamp.into(),
                last_error: None,
            }),
            artifacts: vec![project_store::AutonomousUnitArtifactRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                unit_id,
                attempt_id,
                artifact_id: artifact_id.clone(),
                artifact_kind: "verification_evidence".into(),
                status: project_store::AutonomousUnitArtifactStatus::Recorded,
                summary: "Autonomous attempt blocked on `Terminal input required` and is waiting for operator action.".into(),
                content_hash: None,
                payload: Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(
                    project_store::AutonomousVerificationEvidencePayloadRecord {
                        project_id: project_id.into(),
                        run_id: run_id.into(),
                        unit_id: format!("{run_id}:unit:1"),
                        attempt_id: format!("{run_id}:unit:1:attempt:1"),
                        artifact_id,
                        evidence_kind: "terminal_input_required".into(),
                        label: "Terminal input required".into(),
                        outcome: project_store::AutonomousVerificationOutcomeRecord::Blocked,
                        command_result: None,
                        action_id: Some(action_id.into()),
                        boundary_id: Some(boundary_id.into()),
                    },
                )),
                created_at: timestamp.into(),
                updated_at: timestamp.into(),
            }],
        },
    )
    .expect("seed blocked autonomous run");
}

pub(crate) fn write_json_line<T: Serialize>(stream: &mut TcpStream, value: &T) {
    serde_json::to_writer(&mut *stream, value).expect("write json line");
    stream.write_all(b"\n").expect("write newline");
    stream.flush().expect("flush line");
}
