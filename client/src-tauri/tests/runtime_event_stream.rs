use std::{
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
use cadence_desktop_lib::{
    auth::{now_timestamp, persist_openai_codex_session, StoredOpenAiCodexSession},
    commands::{
        start_runtime_session::start_runtime_session, CommandError, CommandErrorClass,
        ProjectIdRequestDto, ProjectSummaryDto, ProjectUpdateReason, ProjectUpdatedPayloadDto,
        RuntimeAuthPhase, RuntimeRunCheckpointKindDto, RuntimeRunTransportLivenessDto,
        RuntimeSessionDto, RuntimeStreamItemDto, RuntimeStreamItemKind, RuntimeToolCallState,
        SubscribeRuntimeStreamResponseDto, SUBSCRIBE_RUNTIME_STREAM_COMMAND,
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
use serde::Serialize;
use serde_json::{json, Value};
use tauri::Manager;
use tempfile::TempDir;

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

const STRUCTURED_EVENT_PREFIX: &str = "__CADENCE_EVENT__ ";
const STREAM_TIMEOUT: Duration = Duration::from_secs(5);

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

fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cadence-runtime-supervisor"))
}

fn invoke_request(command: &str, payload: Value) -> tauri::webview::InvokeRequest {
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

fn channel_string() -> String {
    serde_json::to_value(tauri::ipc::Channel::<RuntimeStreamItemDto>::new(|_| Ok(())))
        .expect("channel should serialize")
        .as_str()
        .expect("channel should serialize to string")
        .to_string()
}

fn subscribe_request(
    project_id: &str,
    raw_channel: &str,
    item_kinds: &[&str],
) -> tauri::webview::InvokeRequest {
    invoke_request(
        SUBSCRIBE_RUNTIME_STREAM_COMMAND,
        json!({
            "request": {
                "projectId": project_id,
                "channel": raw_channel,
                "itemKinds": item_kinds,
            }
        }),
    )
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

fn current_unix_timestamp() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_secs() as i64
}

fn seed_authenticated_runtime(
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

fn launch_request(
    project_id: &str,
    repo_root: &Path,
    run_id: &str,
    command: &str,
) -> RuntimeSupervisorLaunchRequest {
    let shell = runtime_shell::launch_script(command);
    RuntimeSupervisorLaunchRequest {
        project_id: project_id.into(),
        repo_root: repo_root.to_path_buf(),
        runtime_kind: "openai_codex".into(),
        run_id: run_id.into(),
        session_id: "session-auth".into(),
        flow_id: None,
        program: shell.program,
        args: shell.args,
        startup_timeout: Duration::from_secs(5),
        control_timeout: Duration::from_millis(750),
        supervisor_binary: Some(supervisor_binary_path()),
    }
}

fn probe_request(project_id: &str, repo_root: &Path) -> RuntimeSupervisorProbeRequest {
    RuntimeSupervisorProbeRequest {
        project_id: project_id.into(),
        repo_root: repo_root.to_path_buf(),
        control_timeout: Duration::from_millis(750),
    }
}

fn stop_request(project_id: &str, repo_root: &Path) -> RuntimeSupervisorStopRequest {
    RuntimeSupervisorStopRequest {
        project_id: project_id.into(),
        repo_root: repo_root.to_path_buf(),
        control_timeout: Duration::from_millis(750),
        shutdown_timeout: Duration::from_secs(4),
    }
}

fn launch_supervised_run(
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

fn wait_for_runtime_run(
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

fn stop_supervisor_run(state: &DesktopState, project_id: &str, repo_root: &Path) {
    let stopped = stop_runtime_run(state, stop_request(project_id, repo_root))
        .expect("stop runtime run should succeed")
        .expect("runtime run should exist");
    assert_eq!(stopped.run.project_id, project_id);
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

fn assert_monotonic_sequences(items: &[RuntimeStreamItemDto], expected_run_id: &str) {
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

fn seed_pending_operator_approval(
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

fn seed_terminal_runtime_run(repo_root: &Path, project_id: &str, run_id: &str) {
    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
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
        },
    )
    .expect("seed failed runtime run");
}

fn seed_fake_runtime_run(repo_root: &Path, project_id: &str, run_id: &str, endpoint: &str) {
    let timestamp = now_timestamp();
    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
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
                created_at: timestamp,
            }),
        },
    )
    .expect("seed fake runtime run");
}

fn seed_runtime_action_required(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    session_id: &str,
    boundary_id: &str,
    created_at: &str,
) -> String {
    let runtime_run = project_store::load_runtime_run(repo_root, project_id)
        .expect("load runtime run for action-required seed")
        .expect("runtime run should exist before seeding action-required state");

    let persisted = project_store::upsert_runtime_action_required(
        repo_root,
        &RuntimeActionRequiredUpsertRecord {
            project_id: project_id.into(),
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

fn seed_blocked_autonomous_run(
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
                run_id: run_id.into(),
                runtime_kind: "openai_codex".into(),
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

fn write_json_line<T: Serialize>(stream: &mut TcpStream, value: &T) {
    serde_json::to_writer(&mut *stream, value).expect("write json line");
    stream.write_all(b"\n").expect("write newline");
    stream.flush().expect("flush line");
}

#[test]
fn subscribe_runtime_stream_rejects_missing_channel_and_unsupported_kind_lists_activity() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, _auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to create mock webview window");

    tauri::test::assert_ipc_response(
        &webview,
        invoke_request(
            SUBSCRIBE_RUNTIME_STREAM_COMMAND,
            json!({
                "request": {
                    "projectId": "project-1",
                    "itemKinds": ["transcript"]
                }
            }),
        ),
        Err(CommandError::user_fixable(
            "runtime_stream_channel_missing",
            "Cadence requires a runtime stream channel before it can start streaming selected-project runtime items.",
        )),
    );

    tauri::test::assert_ipc_response(
        &webview,
        subscribe_request("project-1", &channel_string(), &["bogus"]),
        Err(CommandError::user_fixable(
            "runtime_stream_item_kind_unsupported",
            "Cadence does not support runtime stream item kind `bogus`. Allowed kinds: transcript, tool, activity, action_required, complete, failure.",
        )),
    );
}

#[test]
fn subscribe_runtime_stream_fails_closed_without_an_attachable_durable_run() {
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to create mock webview window");

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    tauri::test::assert_ipc_response(
        &webview,
        subscribe_request(&project_id, &channel_string(), &["transcript", "failure"]),
        Err(CommandError {
            code: "runtime_stream_run_unavailable".into(),
            class: CommandErrorClass::Retryable,
            message: "Cadence cannot start a live runtime stream until the selected project has an attachable durable run.".into(),
            retryable: true,
        }),
    );

    seed_terminal_runtime_run(&repo_root, &project_id, "run-failed");

    tauri::test::assert_ipc_response(
        &webview,
        subscribe_request(&project_id, &channel_string(), &["transcript", "failure"]),
        Err(CommandError {
            code: "runtime_stream_run_unavailable".into(),
            class: CommandErrorClass::UserFixable,
            message: "The detached runtime supervisor exited with status 17.".into(),
            retryable: false,
        }),
    );
}

#[test]
fn subscribe_runtime_stream_returns_run_scoped_response_for_an_attachable_run() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let webview = tauri::WebviewWindowBuilder::new(&app, "main", Default::default())
        .build()
        .expect("failed to create mock webview window");

    seed_authenticated_runtime(&app, &auth_store_path, &project_id);
    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-subscribe",
        &runtime_shell::script_print_line_and_sleep("ready", 2),
    );

    tauri::test::assert_ipc_response(
        &webview,
        subscribe_request(
            &project_id,
            &channel_string(),
            &["transcript", "tool", "activity", "complete"],
        ),
        Ok(SubscribeRuntimeStreamResponseDto {
            project_id: project_id.clone(),
            runtime_kind: "openai_codex".into(),
            run_id: launched.run.run_id.clone(),
            session_id: "session-auth".into(),
            flow_id: None,
            subscribed_item_kinds: vec![
                RuntimeStreamItemKind::Transcript,
                RuntimeStreamItemKind::Tool,
                RuntimeStreamItemKind::Activity,
                RuntimeStreamItemKind::Complete,
            ],
        }),
    );

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}

#[test]
fn runtime_stream_replays_real_supervisor_events_after_fresh_host_reload() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let live_lines = vec![
        format!(
            "{STRUCTURED_EVENT_PREFIX}{}",
            json!({
                "kind": "tool",
                "tool_call_id": "tool-1",
                "tool_name": "read",
                "tool_state": "running",
                "detail": "Collecting workspace context",
                "tool_summary": {
                    "kind": "file",
                    "path": "README.md",
                    "scope": null,
                    "lineCount": 12,
                    "matchCount": null,
                    "truncated": true
                }
            })
        ),
        "plain transcript line".to_string(),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"activity\",\"code\":\"phase_progress\",\"title\":\"Planning\",\"detail\":\"Replay buffer ready\"}}"
        ),
    ];

    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-reload",
        &runtime_shell::script_print_lines_and_sleep(&live_lines, 3),
    );

    wait_for_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        |snapshot| {
            snapshot.run.status == RuntimeRunStatus::Running
                && snapshot.last_checkpoint_sequence >= 2
        },
    );

    let (fresh_state, _fresh_registry_path, _fresh_auth_store_path) = create_state(&root);
    let fresh_app = build_mock_app(fresh_state);
    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &fresh_app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::Tool,
            RuntimeStreamItemKind::Activity,
            RuntimeStreamItemKind::Complete,
        ],
        channel,
    );

    let items = collect_until_terminal(receiver);
    assert_monotonic_sequences(&items, &launched.run.run_id);
    assert_eq!(
        items
            .iter()
            .map(|item| item.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            RuntimeStreamItemKind::Tool,
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::Activity,
            RuntimeStreamItemKind::Complete,
        ],
        "unexpected replay items: {items:?}"
    );

    assert!(matches!(
        &items[0],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Tool,
            tool_call_id: Some(tool_call_id),
            tool_name: Some(tool_name),
            tool_state: Some(RuntimeToolCallState::Running),
            detail: Some(detail),
            tool_summary: Some(cadence_desktop_lib::runtime::protocol::ToolResultSummary::File(summary)),
            ..
        } if tool_call_id == "tool-1"
            && tool_name == "read"
            && detail == "Collecting workspace context"
            && summary.path.as_deref() == Some("README.md")
            && summary.line_count == Some(12)
            && summary.truncated
    ));
    assert!(matches!(
        &items[1],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Transcript,
            text: Some(text),
            ..
        } if text == "plain transcript line"
    ));
    assert!(matches!(
        &items[2],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Activity,
            code: Some(code),
            title: Some(title),
            detail: Some(detail),
            ..
        } if code == "phase_progress"
            && title == "Planning"
            && detail == "Replay buffer ready"
    ));
    assert!(matches!(
        &items[3],
        RuntimeStreamItemDto {
            kind: RuntimeStreamItemKind::Complete,
            detail: Some(detail),
            ..
        } if detail.contains("finished")
    ));

    stop_supervisor_run(
        fresh_app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
    );
}

#[test]
fn runtime_stream_appends_pending_action_required_after_replay_with_monotonic_sequence() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-action-required",
        &runtime_shell::script_print_line_and_sleep("backlog ready", 3),
    );

    wait_for_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        |snapshot| {
            snapshot.run.status == RuntimeRunStatus::Running
                && snapshot.last_checkpoint_sequence >= 1
        },
    );
    let action_id = seed_pending_operator_approval(
        &repo_root,
        &project_id,
        &launched.run.run_id,
        runtime.session_id.as_deref().expect("session id"),
    );

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::ActionRequired,
            RuntimeStreamItemKind::Complete,
        ],
        channel,
    );

    let items = collect_until_terminal(receiver);
    assert_monotonic_sequences(&items, &launched.run.run_id);
    assert_eq!(
        items
            .iter()
            .map(|item| item.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::ActionRequired,
            RuntimeStreamItemKind::Complete,
        ]
    );
    assert_eq!(items[0].text.as_deref(), Some("backlog ready"));
    assert_eq!(items[1].action_id.as_deref(), Some(action_id.as_str()));
    assert_eq!(items[1].boundary_id.as_deref(), Some("boundary-pending"));
    assert_eq!(
        items[1].action_type.as_deref(),
        Some("terminal_input_required")
    );
    assert_eq!(items[1].title.as_deref(), Some("Terminal input required"));
    assert_eq!(
        items[1].detail.as_deref(),
        Some("Provide terminal input to continue this run.")
    );

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}

#[test]
fn runtime_stream_dedupes_replayed_action_required_against_durable_pending_queue() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fake supervisor listener");
    let endpoint = listener
        .local_addr()
        .expect("read fake supervisor endpoint")
        .to_string();

    seed_fake_runtime_run(
        &repo_root,
        &project_id,
        "run-dedupe-action-required",
        &endpoint,
    );

    let action_id = seed_runtime_action_required(
        &repo_root,
        &project_id,
        "run-dedupe-action-required",
        runtime.session_id.as_deref().expect("session id"),
        "boundary-1",
        "2026-04-15T23:10:02Z",
    );
    seed_blocked_autonomous_run(
        &repo_root,
        &project_id,
        "run-dedupe-action-required",
        &action_id,
        "boundary-1",
    );

    let server = thread::spawn({
        let project_id = project_id.clone();
        let action_id = action_id.clone();
        move || {
            let (mut stream, _) = listener.accept().expect("accept fake supervisor attach");
            let mut line = String::new();
            BufReader::new(stream.try_clone().expect("clone fake supervisor stream"))
                .read_line(&mut line)
                .expect("read attach request");
            let request: SupervisorControlRequest =
                serde_json::from_str(line.trim()).expect("decode attach request");
            assert!(matches!(
                request,
                SupervisorControlRequest::Attach {
                    project_id: requested_project_id,
                    run_id,
                    after_sequence: None,
                    ..
                } if requested_project_id == project_id && run_id == "run-dedupe-action-required"
            ));

            write_json_line(
                &mut stream,
                &SupervisorControlResponse::Attached {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id: project_id.clone(),
                    run_id: "run-dedupe-action-required".into(),
                    after_sequence: None,
                    replayed_count: 1,
                    replay_truncated: false,
                    oldest_available_sequence: Some(1),
                    latest_sequence: Some(1),
                },
            );
            write_json_line(
                &mut stream,
                &SupervisorControlResponse::Event {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id,
                    run_id: "run-dedupe-action-required".into(),
                    sequence: 1,
                    created_at: "2026-04-15T23:10:02Z".into(),
                    replay: true,
                    item: SupervisorLiveEventPayload::ActionRequired {
                        action_id,
                        boundary_id: "boundary-1".into(),
                        action_type: "terminal_input_required".into(),
                        title: "Terminal input required".into(),
                        detail: "Provide terminal input to continue this run.".into(),
                    },
                },
            );
            thread::sleep(Duration::from_millis(150));
        }
    });

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        "run-dedupe-action-required",
        vec![
            RuntimeStreamItemKind::ActionRequired,
            RuntimeStreamItemKind::Failure,
        ],
        channel,
    );

    let items = collect_until_terminal(receiver);
    let action_required_items = items
        .iter()
        .filter(|item| item.kind == RuntimeStreamItemKind::ActionRequired)
        .collect::<Vec<_>>();

    assert_eq!(
        action_required_items.len(),
        1,
        "expected one deduped action-required item, got {items:?}"
    );
    assert_eq!(
        action_required_items[0].action_id.as_deref(),
        Some(action_id.as_str())
    );
    assert_eq!(
        action_required_items[0].boundary_id.as_deref(),
        Some("boundary-1")
    );

    let autonomous_snapshot = project_store::load_autonomous_run(&repo_root, &project_id)
        .expect("load autonomous run after replayed runtime stream")
        .expect("autonomous run should still exist after replayed runtime stream");
    let blocked_evidence = autonomous_snapshot
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.action_id.as_deref() == Some(action_id.as_str())
                        && payload.boundary_id.as_deref() == Some("boundary-1")
                        && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
            )
        })
        .count();
    assert_eq!(blocked_evidence, 1);

    server.join().expect("join fake supervisor thread");
}

#[test]
fn runtime_stream_dropped_channel_does_not_poison_resubscribe() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-dropped-channel",
        &runtime_shell::script_print_lines_and_sleep(
            &[
                "first replay line".to_string(),
                "second replay line".to_string(),
            ],
            3,
        ),
    );

    wait_for_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        |snapshot| {
            snapshot.run.status == RuntimeRunStatus::Running
                && snapshot.last_checkpoint_sequence >= 1
        },
    );

    let delivery_attempts = Mutex::new(0_usize);
    let dropped_channel = tauri::ipc::Channel::<RuntimeStreamItemDto>::new(move |_body| {
        let mut attempts = delivery_attempts.lock().expect("delivery attempts lock");
        *attempts += 1;
        if *attempts >= 2 {
            Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "channel dropped").into())
        } else {
            Ok(())
        }
    });

    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::Complete,
            RuntimeStreamItemKind::Failure,
        ],
        dropped_channel,
    );

    thread::sleep(Duration::from_millis(250));

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::Complete,
        ],
        channel,
    );

    let items = collect_until_terminal(receiver);
    assert_monotonic_sequences(&items, &launched.run.run_id);
    assert_eq!(
        items
            .iter()
            .map(|item| item.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::Transcript,
            RuntimeStreamItemKind::Complete,
        ]
    );
    assert_eq!(items[0].text.as_deref(), Some("first replay line"));
    assert_eq!(items[1].text.as_deref(), Some("second replay line"));

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}

#[test]
fn runtime_stream_redacts_secret_bearing_replay_without_leaking_tokens() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let access_token = jwt_with_account_id("acct-1");
    let live_lines = vec![
        "access_token=shh-secret-value".to_string(),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"activity\",\"code\":\"diag\",\"title\":\"Auth\",\"detail\":\"Bearer hidden-token\"}}"
        ),
    ];

    let launched = launch_supervised_run(
        app.state::<DesktopState>().inner(),
        &project_id,
        &repo_root,
        "run-redaction-bridge",
        &runtime_shell::script_print_lines_and_sleep(&live_lines, 3),
    );

    wait_for_runtime_run(
        app.state::<DesktopState>().inner(),
        &repo_root,
        &project_id,
        |snapshot| {
            snapshot.run.status == RuntimeRunStatus::Running
                && snapshot.last_checkpoint_sequence >= 2
        },
    );

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        &launched.run.run_id,
        vec![
            RuntimeStreamItemKind::Activity,
            RuntimeStreamItemKind::Complete,
        ],
        channel,
    );

    let items = collect_until_terminal(receiver);
    assert_monotonic_sequences(&items, &launched.run.run_id);
    assert_eq!(
        items
            .iter()
            .map(|item| item.kind.clone())
            .collect::<Vec<_>>(),
        vec![
            RuntimeStreamItemKind::Activity,
            RuntimeStreamItemKind::Activity,
            RuntimeStreamItemKind::Complete,
        ]
    );
    assert!(items[..2].iter().all(|item| {
        item.code.as_deref() == Some("runtime_supervisor_live_event_redacted")
            && item.title.as_deref() == Some("Live output redacted")
    }));

    let serialized_items = serde_json::to_string(&items).expect("serialize bridged items");
    assert!(!serialized_items.contains("access_token"));
    assert!(!serialized_items.contains("Bearer"));
    assert!(!serialized_items.contains("sk-"));
    assert!(!serialized_items.contains("refresh-1"));
    assert!(!serialized_items.contains(&access_token));

    let persisted = project_store::load_runtime_run(&repo_root, &project_id)
        .expect("load stored runtime run")
        .expect("stored runtime run should exist");
    let checkpoint_dump = persisted
        .checkpoints
        .iter()
        .map(|checkpoint| checkpoint.summary.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!checkpoint_dump.contains("access_token"));
    assert!(!checkpoint_dump.contains("Bearer"));
    assert!(!checkpoint_dump.contains("refresh-1"));
    assert!(!checkpoint_dump.contains(&access_token));
    assert!(checkpoint_dump.contains("runtime_supervisor_live_event_redacted"));

    stop_supervisor_run(app.state::<DesktopState>().inner(), &project_id, &repo_root);
}

#[test]
fn runtime_stream_emits_typed_failure_when_supervisor_sequence_is_invalid() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let (state, _registry_path, auth_store_path) = create_state(&root);
    let app = build_mock_app(state);
    let (project_id, repo_root) = seed_project(&root, &app);
    let runtime = seed_authenticated_runtime(&app, &auth_store_path, &project_id);

    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind fake supervisor listener");
    let endpoint = listener
        .local_addr()
        .expect("read fake supervisor endpoint")
        .to_string();
    let server = thread::spawn({
        let project_id = project_id.clone();
        move || {
            let (mut stream, _) = listener.accept().expect("accept fake supervisor attach");
            let mut line = String::new();
            BufReader::new(stream.try_clone().expect("clone fake supervisor stream"))
                .read_line(&mut line)
                .expect("read attach request");
            let request: SupervisorControlRequest =
                serde_json::from_str(line.trim()).expect("decode attach request");
            assert!(matches!(
                request,
                SupervisorControlRequest::Attach {
                    project_id: requested_project_id,
                    run_id,
                    after_sequence: None,
                    ..
                } if requested_project_id == project_id && run_id == "run-invalid-sequence"
            ));

            write_json_line(
                &mut stream,
                &SupervisorControlResponse::Attached {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id: project_id.clone(),
                    run_id: "run-invalid-sequence".into(),
                    after_sequence: None,
                    replayed_count: 1,
                    replay_truncated: false,
                    oldest_available_sequence: Some(1),
                    latest_sequence: Some(1),
                },
            );
            write_json_line(
                &mut stream,
                &SupervisorControlResponse::Event {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id,
                    run_id: "run-invalid-sequence".into(),
                    sequence: 0,
                    created_at: "2026-04-15T23:10:02Z".into(),
                    replay: true,
                    item: SupervisorLiveEventPayload::Transcript {
                        text: "bad sequence".into(),
                    },
                },
            );
            thread::sleep(Duration::from_millis(250));
        }
    });

    seed_fake_runtime_run(&repo_root, &project_id, "run-invalid-sequence", &endpoint);

    let (channel, receiver) = capture_stream_channel();
    start_direct_runtime_stream(
        &app,
        &project_id,
        &repo_root,
        &runtime,
        "run-invalid-sequence",
        vec![RuntimeStreamItemKind::Failure],
        channel,
    );

    let items = collect_until_terminal(receiver);
    eprintln!("invalid sequence items: {items:?}");
    assert_eq!(
        items.len(),
        1,
        "expected a single failure item, got {items:?}"
    );
    let failure = &items[0];
    assert_eq!(failure.kind, RuntimeStreamItemKind::Failure);
    assert_eq!(
        failure.code.as_deref(),
        Some("runtime_stream_sequence_invalid")
    );
    assert_eq!(failure.retryable, Some(false));
    assert!(failure
        .message
        .as_deref()
        .expect("failure message")
        .contains("sequence 0"));

    server.join().expect("join fake supervisor thread");
}

#[test]
fn runtime_stream_contract_serialization_exposes_run_id_sequence_and_activity() {
    let stream_item = serde_json::to_value(RuntimeStreamItemDto {
        kind: RuntimeStreamItemKind::Activity,
        run_id: "run-1".into(),
        sequence: 7,
        session_id: Some("session-1".into()),
        flow_id: Some("flow-1".into()),
        text: None,
        tool_call_id: None,
        tool_name: None,
        tool_state: None,
        tool_summary: None,
        action_id: None,
        boundary_id: None,
        action_type: None,
        title: Some("Planning".into()),
        detail: Some("Replay buffer ready".into()),
        code: Some("phase_progress".into()),
        message: None,
        retryable: None,
        created_at: "2026-04-15T23:10:02Z".into(),
    })
    .expect("serialize runtime stream activity item");

    assert_eq!(
        stream_item,
        json!({
            "kind": "activity",
            "runId": "run-1",
            "sequence": 7,
            "sessionId": "session-1",
            "flowId": "flow-1",
            "text": null,
            "toolCallId": null,
            "toolName": null,
            "toolState": null,
            "actionId": null,
            "boundaryId": null,
            "actionType": null,
            "title": "Planning",
            "detail": "Replay buffer ready",
            "code": "phase_progress",
            "message": null,
            "retryable": null,
            "createdAt": "2026-04-15T23:10:02Z"
        })
    );

    assert_eq!(
        RuntimeStreamItemDto::allowed_kind_names(),
        &[
            "transcript",
            "tool",
            "activity",
            "action_required",
            "complete",
            "failure",
        ]
    );

    let project_updated = serde_json::to_value(ProjectUpdatedPayloadDto {
        project: ProjectSummaryDto {
            id: "project-1".into(),
            name: "cadence".into(),
            description: "Desktop shell".into(),
            milestone: "M004".into(),
            total_phases: 4,
            completed_phases: 2,
            active_phase: 3,
            branch: Some("main".into()),
            runtime: Some("openai_codex".into()),
        },
        reason: ProjectUpdateReason::MetadataChanged,
    })
    .expect("serialize project updated payload");

    assert_eq!(project_updated["project"]["activePhase"], json!(3));
    assert_eq!(project_updated["project"]["completedPhases"], json!(2));
    assert_eq!(project_updated["reason"], json!("metadata_changed"));

    let checkpoint = serde_json::to_value(cadence_desktop_lib::commands::RuntimeRunCheckpointDto {
        sequence: 3,
        kind: RuntimeRunCheckpointKindDto::ActionRequired,
        summary: "Approval required".into(),
        created_at: "2026-04-15T23:10:03Z".into(),
    })
    .expect("serialize runtime checkpoint");
    assert_eq!(checkpoint["sequence"], json!(3));

    let transport = serde_json::to_value(cadence_desktop_lib::commands::RuntimeRunTransportDto {
        kind: "tcp".into(),
        endpoint: "127.0.0.1:45123".into(),
        liveness: RuntimeRunTransportLivenessDto::Reachable,
    })
    .expect("serialize runtime transport");
    assert_eq!(transport["liveness"], json!("reachable"));
}
