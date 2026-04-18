use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
    thread,
    time::{Duration, Instant},
};

use cadence_desktop_lib::{
    db::{self, project_store},
    git::repository::CanonicalRepository,
    runtime::protocol::{
        SupervisorControlRequest, SupervisorControlResponse, SupervisorLiveEventPayload,
        SupervisorToolCallState, SUPERVISOR_PROTOCOL_VERSION,
    },
    runtime::{
        autonomous_orchestrator::persist_supervisor_event, launch_detached_runtime_supervisor,
        probe_runtime_run, stop_runtime_run, submit_runtime_run_input,
        RuntimeSupervisorLaunchRequest, RuntimeSupervisorProbeRequest,
        RuntimeSupervisorStopRequest, RuntimeSupervisorSubmitInputRequest,
    },
    state::DesktopState,
};
use tempfile::TempDir;

#[path = "support/runtime_shell.rs"]
mod runtime_shell;

const STRUCTURED_EVENT_PREFIX: &str = "__CADENCE_EVENT__ ";
const ATTACH_READ_TIMEOUT: Duration = Duration::from_secs(2);

fn supervisor_test_guard() -> MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn seed_project(root: &TempDir, project_id: &str, repository_id: &str, repo_name: &str) -> PathBuf {
    let repo_root = root.path().join(repo_name);
    std::fs::create_dir_all(&repo_root).expect("create repo root");
    let canonical_root = std::fs::canonicalize(&repo_root).expect("canonical repo root");
    let root_path_string = canonical_root.to_string_lossy().into_owned();

    let repository = CanonicalRepository {
        project_id: project_id.into(),
        repository_id: repository_id.into(),
        root_path: canonical_root.clone(),
        root_path_string,
        common_git_dir: canonical_root.join(".git"),
        display_name: repo_name.into(),
        branch_name: Some("main".into()),
        head_sha: Some("abc123".into()),
        branch: None,
        status_entries: Vec::new(),
        has_staged_changes: false,
        has_unstaged_changes: false,
        has_untracked_changes: false,
    };

    let state = DesktopState::default();
    db::import_project(&repository, state.import_failpoints()).expect("import project");

    canonical_root
}

fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_cadence-runtime-supervisor"))
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
        session_id: "session-1".into(),
        flow_id: Some("flow-1".into()),
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

fn seed_running_runtime_run(repo_root: &Path, project_id: &str, run_id: &str, endpoint: &str) {
    let now = cadence_desktop_lib::auth::now_timestamp();

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
                    endpoint: endpoint.into(),
                    liveness: project_store::RuntimeRunTransportLiveness::Reachable,
                },
                started_at: now.clone(),
                last_heartbeat_at: Some(now.clone()),
                stopped_at: None,
                last_error: None,
                updated_at: now,
            },
            checkpoint: None,
        },
    )
    .expect("seed running runtime run");
}

fn seed_active_autonomous_run(repo_root: &Path, project_id: &str, run_id: &str) {
    let timestamp = "2026-04-16T12:00:00Z";
    let payload = project_store::AutonomousRunUpsertRecord {
        run: project_store::AutonomousRunRecord {
            project_id: project_id.into(),
            run_id: run_id.into(),
            runtime_kind: "openai_codex".into(),
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

    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    loop {
        match project_store::upsert_autonomous_run(repo_root, &payload) {
            Ok(_) => return,
            Err(_) if std::time::Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(50))
            }
            Err(error) => panic!("seed active autonomous run: {error:?}"),
        }
    }
}

fn spawn_single_response_control_server(response_line: String) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock control listener");
    let endpoint = listener
        .local_addr()
        .expect("read mock control listener addr")
        .to_string();

    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept mock control connection");
        let mut request_line = String::new();
        let mut reader = BufReader::new(
            stream
                .try_clone()
                .expect("clone mock control stream for request read"),
        );
        let _ = reader.read_line(&mut request_line);

        stream
            .write_all(response_line.as_bytes())
            .expect("write mock control response");
        stream
            .write_all(b"\n")
            .expect("write mock control response newline");
        stream.flush().expect("flush mock control response");
    });

    (endpoint, handle)
}

fn wait_for_runtime_run(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    predicate: impl Fn(&project_store::RuntimeRunSnapshotRecord) -> bool,
) -> project_store::RuntimeRunSnapshotRecord {
    let deadline = Instant::now() + Duration::from_secs(6);

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

fn attach_reader(endpoint: &str, request: SupervisorControlRequest) -> BufReader<TcpStream> {
    let mut stream = TcpStream::connect(endpoint).expect("connect attach socket");
    stream
        .set_read_timeout(Some(ATTACH_READ_TIMEOUT))
        .expect("set attach read timeout");
    stream
        .set_write_timeout(Some(ATTACH_READ_TIMEOUT))
        .expect("set attach write timeout");
    serde_json::to_writer(&mut stream, &request).expect("write attach request");
    stream.write_all(b"\n").expect("write attach newline");
    stream.flush().expect("flush attach request");
    BufReader::new(stream)
}

fn send_control_request(
    endpoint: &str,
    request: SupervisorControlRequest,
) -> SupervisorControlResponse {
    let mut reader = attach_reader(endpoint, request);
    read_supervisor_response(&mut reader)
}

fn read_supervisor_response(reader: &mut BufReader<TcpStream>) -> SupervisorControlResponse {
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .expect("read supervisor response");
    assert!(bytes > 0, "expected a supervisor response frame");
    serde_json::from_str(line.trim()).expect("decode supervisor response")
}

#[derive(Debug, Clone, Copy)]
struct AttachAck {
    replayed_count: usize,
    replay_truncated: bool,
    oldest_available_sequence: Option<u64>,
    latest_sequence: Option<u64>,
}

fn expect_attach_ack(response: SupervisorControlResponse) -> AttachAck {
    match response {
        SupervisorControlResponse::Attached {
            replayed_count,
            replay_truncated,
            oldest_available_sequence,
            latest_sequence,
            ..
        } => AttachAck {
            replayed_count: replayed_count as usize,
            replay_truncated,
            oldest_available_sequence,
            latest_sequence,
        },
        other => panic!("expected attach ack, got {other:?}"),
    }
}

fn read_event_frames(
    reader: &mut BufReader<TcpStream>,
    replayed_count: usize,
) -> Vec<SupervisorControlResponse> {
    (0..replayed_count)
        .map(|_| read_supervisor_response(reader))
        .collect()
}

fn assert_monotonic_sequences(frames: &[SupervisorControlResponse], expected_run_id: &str) {
    let mut previous = None;
    for frame in frames {
        match frame {
            SupervisorControlResponse::Event {
                run_id,
                sequence,
                replay,
                ..
            } => {
                assert_eq!(run_id, expected_run_id);
                assert!(*replay, "expected replayed event frame, got {frame:?}");
                if let Some(previous) = previous {
                    assert!(
                        *sequence > previous,
                        "expected monotonic sequence ordering, got {previous} then {sequence}"
                    );
                }
                previous = Some(*sequence);
            }
            other => panic!("expected event frame, got {other:?}"),
        }
    }
}

fn response_dump(frames: &[SupervisorControlResponse]) -> String {
    frames
        .iter()
        .map(|frame| serde_json::to_string(frame).expect("serialize frame"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn detached_supervisor_launches_and_recovers_after_fresh_host_probe() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-1",
            &runtime_shell::script_print_line_and_sleep("hello from detached supervisor", 5),
        ),
    )
    .expect("launch detached runtime supervisor");

    assert_eq!(
        launched.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert!(!launched.run.transport.endpoint.is_empty());
    assert_eq!(
        state
            .runtime_supervisor_controller()
            .snapshot(project_id)
            .as_ref()
            .map(|snapshot| snapshot.run_id.as_str()),
        Some("run-1")
    );

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.run.transport.liveness
                == project_store::RuntimeRunTransportLiveness::Reachable
            && snapshot.last_checkpoint_sequence >= 1
    });
    assert!(running.run.last_heartbeat_at.is_some());

    let fresh_state = DesktopState::default();
    let recovered = probe_runtime_run(&fresh_state, probe_request(project_id, &repo_root))
        .expect("probe with fresh host state")
        .expect("runtime run should still exist");
    assert_eq!(recovered.run.run_id, "run-1");
    assert_eq!(
        recovered.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert_eq!(
        recovered.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Reachable
    );
    assert!(recovered.run.last_heartbeat_at.is_some());
    assert!(recovered.last_checkpoint_sequence >= 1);

    let stopped = stop_runtime_run(&fresh_state, stop_request(project_id, &repo_root))
        .expect("stop detached runtime supervisor")
        .expect("stopped runtime run should exist");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
    assert!(stopped.run.stopped_at.is_some());
}

#[test]
fn detached_supervisor_probe_marks_unreachable_run_stale() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");

    project_store::upsert_runtime_run(
        &repo_root,
        &project_store::RuntimeRunUpsertRecord {
            run: project_store::RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: "run-stale".into(),
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
            checkpoint: None,
        },
    )
    .expect("seed unreachable runtime run");

    let state = DesktopState::default();
    let recovered = probe_runtime_run(&state, probe_request(project_id, &repo_root))
        .expect("probe stale runtime run")
        .expect("runtime run should exist after stale probe");

    assert_eq!(recovered.run.status, project_store::RuntimeRunStatus::Stale);
    assert_eq!(
        recovered.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Unreachable
    );
    assert_eq!(
        recovered
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_connect_failed")
    );
}

#[test]
fn detached_supervisor_rejects_missing_shell_program() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let state = DesktopState::default();

    let error = launch_detached_runtime_supervisor(
        &state,
        RuntimeSupervisorLaunchRequest {
            project_id: project_id.into(),
            repo_root,
            runtime_kind: "openai_codex".into(),
            run_id: "run-invalid".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            program: String::new(),
            args: Vec::new(),
            startup_timeout: Duration::from_secs(5),
            control_timeout: Duration::from_millis(750),
            supervisor_binary: Some(supervisor_binary_path()),
        },
    )
    .expect_err("missing shell program should fail");

    assert_eq!(error.code, "invalid_request");
}

#[test]
fn detached_supervisor_rejects_duplicate_running_project_launches() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-1",
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect("launch first detached runtime supervisor");
    assert_eq!(
        launched.run.status,
        project_store::RuntimeRunStatus::Running
    );

    let error = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-2",
            &runtime_shell::script_sleep(5),
        ),
    )
    .expect_err("duplicate launch should fail");
    assert_eq!(error.code, "runtime_run_already_active");

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop first detached runtime supervisor")
        .expect("runtime run should exist after stop");
    assert!(matches!(
        stopped.run.status,
        project_store::RuntimeRunStatus::Stopped | project_store::RuntimeRunStatus::Stale
    ));
}

#[test]
fn detached_supervisor_marks_fast_nonzero_exit_as_failed_without_live_attach() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-1";
    let repo_root = seed_project(&root, project_id, "repo-1", "repo");
    let state = DesktopState::default();

    launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-fast-exit",
            &runtime_shell::script_exit(17),
        ),
    )
    .expect("launch fast-exit detached runtime supervisor");

    let terminal = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Failed
    });
    assert_eq!(terminal.run.run_id, "run-fast-exit");
    assert_eq!(
        terminal
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_exit_nonzero")
    );
    assert!(
        terminal.run.transport.liveness == project_store::RuntimeRunTransportLiveness::Reachable
    );
}

#[test]
fn detached_supervisor_attach_replays_buffered_events_after_fresh_host_probe() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-attach";
    let repo_root = seed_project(&root, project_id, "repo-attach", "repo");
    let state = DesktopState::default();

    let live_lines = vec![
        format!(
            "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"tool\",\"tool_call_id\":\"tool-1\",\"tool_name\":\"inspect_repository\",\"tool_state\":\"running\",\"detail\":\"Collecting workspace context\"}}"
        ),
        "plain transcript line".to_string(),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"activity\",\"code\":\"phase_progress\",\"title\":\"Planning\",\"detail\":\"Replay buffer ready\"}}"
        ),
    ];

    launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-attach",
            &runtime_shell::script_print_lines_and_sleep(&live_lines, 5),
        ),
    )
    .expect("launch attachable runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.last_checkpoint_sequence >= 2
    });

    let fresh_state = DesktopState::default();
    let recovered = probe_runtime_run(&fresh_state, probe_request(project_id, &repo_root))
        .expect("probe with fresh state")
        .expect("runtime run should exist");

    let mut reader = attach_reader(
        &recovered.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-attach", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert_eq!(attached.replayed_count, 3);
    assert_eq!(attached.latest_sequence, Some(3));
    assert_eq!(attached.oldest_available_sequence, Some(1));
    assert!(!attached.replay_truncated);

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert_monotonic_sequences(&frames, "run-attach");
    assert!(matches!(
        &frames[0],
        SupervisorControlResponse::Event {
            item:
                SupervisorLiveEventPayload::Tool {
                    tool_call_id,
                    tool_name,
                    tool_state: SupervisorToolCallState::Running,
                    detail,
                },
            ..
        } if tool_call_id == "tool-1"
            && tool_name == "inspect_repository"
            && detail.as_deref() == Some("Collecting workspace context")
    ));
    assert!(matches!(
        &frames[1],
        SupervisorControlResponse::Event {
            item: SupervisorLiveEventPayload::Transcript { text },
            ..
        } if text == "plain transcript line"
    ));
    assert!(matches!(
        &frames[2],
        SupervisorControlResponse::Event {
            item: SupervisorLiveEventPayload::Activity { code, title, detail },
            ..
        } if code == "phase_progress"
            && title == "Planning"
            && detail.as_deref() == Some("Replay buffer ready")
    ));

    let stopped = stop_runtime_run(&fresh_state, stop_request(project_id, &repo_root))
        .expect("stop attachable runtime supervisor")
        .expect("stopped runtime run should exist");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn detached_supervisor_attach_rejects_identity_mismatch_without_mutating_run() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-mismatch";
    let repo_root = seed_project(&root, project_id, "repo-mismatch", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-identity",
            &runtime_shell::script_print_line_and_sleep("ready", 5),
        ),
    )
    .expect("launch mismatch runtime supervisor");

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "wrong-run", None),
    );
    let response = read_supervisor_response(&mut reader);
    assert!(matches!(
        response,
        SupervisorControlResponse::Error { code, retryable, .. }
        if code == "runtime_supervisor_identity_mismatch" && !retryable
    ));

    let recovered = probe_runtime_run(&state, probe_request(project_id, &repo_root))
        .expect("probe after mismatch attach")
        .expect("runtime run should still exist");
    assert_eq!(recovered.run.run_id, "run-identity");
    assert_eq!(
        recovered.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert!(recovered.run.last_error.is_none());

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop mismatch runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn detached_supervisor_attach_rejects_invalid_cursor_without_mutating_run() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-cursor";
    let repo_root = seed_project(&root, project_id, "repo-cursor", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-cursor",
            &runtime_shell::script_print_line_and_sleep("ready", 5),
        ),
    )
    .expect("launch cursor runtime supervisor");

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-cursor", Some(0)),
    );
    let response = read_supervisor_response(&mut reader);
    assert!(matches!(
        response,
        SupervisorControlResponse::Error { code, retryable, .. }
        if code == "runtime_supervisor_attach_cursor_invalid" && !retryable
    ));

    let recovered = probe_runtime_run(&state, probe_request(project_id, &repo_root))
        .expect("probe after invalid cursor attach")
        .expect("runtime run should still exist");
    assert_eq!(recovered.run.run_id, "run-cursor");
    assert_eq!(
        recovered.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert!(recovered.run.last_error.is_none());

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop cursor runtime supervisor")
        .expect("runtime run should exist after stop");
    assert!(matches!(
        stopped.run.status,
        project_store::RuntimeRunStatus::Stopped | project_store::RuntimeRunStatus::Stale
    ));
}

#[test]
fn detached_supervisor_attach_replays_only_bounded_ring_window() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-ring";
    let repo_root = seed_project(&root, project_id, "repo-ring", "repo");
    let state = DesktopState::default();
    let emitted_lines = 160_u32;

    let emitted_runtime_lines = (1..=emitted_lines)
        .map(|index| format!("line-{index:03}"))
        .collect::<Vec<_>>();
    let command = runtime_shell::script_print_lines_and_sleep(&emitted_runtime_lines, 5);

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(project_id, &repo_root, "run-ring", &command),
    )
    .expect("launch ring runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.last_checkpoint_sequence >= 10
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-ring", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert_eq!(attached.replayed_count, 128);
    assert!(attached.replay_truncated);
    assert_eq!(attached.oldest_available_sequence, Some(33));
    assert_eq!(attached.latest_sequence, Some(160));

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert_monotonic_sequences(&frames, "run-ring");
    assert!(matches!(
        &frames.first(),
        Some(SupervisorControlResponse::Event {
            sequence,
            item: SupervisorLiveEventPayload::Transcript { text },
            ..
        }) if *sequence == 33 && text == "line-033"
    ));
    assert!(matches!(
        &frames.last(),
        Some(SupervisorControlResponse::Event {
            sequence,
            item: SupervisorLiveEventPayload::Transcript { text },
            ..
        }) if *sequence == 160 && text == "line-160"
    ));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop ring runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn detached_supervisor_live_event_redacts_secret_bearing_output_in_replay_and_checkpoint() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-redaction";
    let repo_root = seed_project(&root, project_id, "repo-redaction", "repo");
    let state = DesktopState::default();

    let live_lines = vec![
        "access_token=shh-secret-value".to_string(),
        format!(
            "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"activity\",\"code\":\"diag\",\"title\":\"Auth\",\"detail\":\"Bearer hidden-token\"}}"
        ),
    ];

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-redaction",
            &runtime_shell::script_print_lines_and_sleep(&live_lines, 5),
        ),
    )
    .expect("launch redaction runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.last_checkpoint_sequence >= 2
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-redaction", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert_eq!(attached.replayed_count, 2);

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert_monotonic_sequences(&frames, "run-redaction");
    assert!(frames.iter().all(|frame| matches!(
        frame,
        SupervisorControlResponse::Event {
            item: SupervisorLiveEventPayload::Activity { code, .. },
            ..
        } if code == "runtime_supervisor_live_event_redacted"
    )));

    let replay_dump = response_dump(&frames);
    assert!(!replay_dump.contains("access_token"));
    assert!(!replay_dump.contains("Bearer"));
    assert!(!replay_dump.contains("sk-"));

    let stored = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load stored runtime run")
        .expect("stored runtime run should exist");
    let checkpoint_dump = stored
        .checkpoints
        .iter()
        .map(|checkpoint| checkpoint.summary.clone())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!checkpoint_dump.contains("access_token"));
    assert!(!checkpoint_dump.contains("Bearer"));
    assert!(checkpoint_dump.contains("runtime_supervisor_live_event_redacted"));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop redaction runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn detached_supervisor_live_event_drops_unsupported_structured_payload_kind() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-invalid-payload";
    let repo_root = seed_project(&root, project_id, "repo-invalid-payload", "repo");
    let state = DesktopState::default();

    let live_lines = vec![format!(
        "{STRUCTURED_EVENT_PREFIX}{{\"kind\":\"mystery\",\"detail\":\"unexpected\"}}"
    )];

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-invalid-payload",
            &runtime_shell::script_print_lines_and_sleep(&live_lines, 5),
        ),
    )
    .expect("launch invalid payload runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.last_checkpoint_sequence >= 1
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-invalid-payload", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert_eq!(attached.replayed_count, 1);

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    assert!(matches!(
        &frames[0],
        SupervisorControlResponse::Event {
            item: SupervisorLiveEventPayload::Activity { code, title, .. },
            ..
        } if code == "runtime_supervisor_live_event_unsupported"
            && title == "Live output fragment dropped"
    ));

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop invalid payload runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn detached_supervisor_attach_rejects_finished_run() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-finished";
    let repo_root = seed_project(&root, project_id, "repo-finished", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-finished",
            &runtime_shell::script_print_line_then_exit("done", 0),
        ),
    )
    .expect("launch finished runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Stopped
    });

    match TcpStream::connect(&launched.run.transport.endpoint) {
        Ok(mut stream) => {
            stream
                .set_read_timeout(Some(ATTACH_READ_TIMEOUT))
                .expect("set attach read timeout");
            stream
                .set_write_timeout(Some(ATTACH_READ_TIMEOUT))
                .expect("set attach write timeout");
            serde_json::to_writer(
                &mut stream,
                &SupervisorControlRequest::attach(project_id, "run-finished", None),
            )
            .expect("write finished attach request");
            stream.write_all(b"\n").expect("write attach newline");
            stream.flush().expect("flush attach request");
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => {}
                Ok(_) => {
                    let response: SupervisorControlResponse =
                        serde_json::from_str(line.trim()).expect("decode finished attach response");
                    assert!(matches!(
                        response,
                        SupervisorControlResponse::Error { code, retryable, .. }
                        if code == "runtime_supervisor_attach_unavailable" && !retryable
                    ));
                }
                Err(error) => {
                    assert!(matches!(
                        error.kind(),
                        std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof
                    ));
                }
            }
        }
        Err(error) => {
            assert_eq!(error.kind(), std::io::ErrorKind::ConnectionRefused);
        }
    }
}

#[test]
fn detached_supervisor_persists_redacted_interactive_boundary_and_replays_same_action_identity() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-interactive";
    let repo_root = seed_project(&root, project_id, "repo-interactive", "repo");
    let state = DesktopState::default();

    let _launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-interactive",
            &runtime_shell::script_prompt_and_sleep("Paste access_token now: ", 5),
        ),
    )
    .expect("launch interactive runtime supervisor");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let interactive_checkpoint = running
        .checkpoints
        .iter()
        .find(|checkpoint| {
            checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
        })
        .expect("action required checkpoint");
    assert!(!interactive_checkpoint.summary.contains("access_token"));
    assert!(!interactive_checkpoint
        .summary
        .contains("Paste access_token now"));
    assert_eq!(
        interactive_checkpoint.summary,
        "Detached runtime blocked on terminal input and is awaiting operator approval."
    );

    let project_snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load project snapshot")
        .snapshot;
    assert_eq!(project_snapshot.approval_requests.len(), 1);
    let approval = &project_snapshot.approval_requests[0];
    assert_eq!(
        approval.status,
        cadence_desktop_lib::commands::OperatorApprovalStatus::Pending
    );
    assert_eq!(approval.session_id.as_deref(), Some("session-1"));
    assert_eq!(approval.flow_id.as_deref(), Some("flow-1"));
    assert_eq!(approval.action_type, "terminal_input_required");
    assert_eq!(approval.title, "Terminal input required");
    assert_eq!(approval.detail, "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.");
    assert!(!approval.detail.contains("access_token"));
    assert!(approval
        .action_id
        .contains(":run:run-interactive:boundary:"));

    let fresh_state = DesktopState::default();
    let recovered = probe_runtime_run(&fresh_state, probe_request(project_id, &repo_root))
        .expect("probe with fresh host state")
        .expect("runtime run should still exist");

    let mut reader = attach_reader(
        &recovered.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-interactive", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    assert!(
        attached.replayed_count >= 1,
        "expected at least one replay frame for interactive boundary"
    );

    let frames = read_event_frames(&mut reader, attached.replayed_count);
    let action_required_frame = frames
        .iter()
        .find(|frame| {
            matches!(
                frame,
                SupervisorControlResponse::Event {
                    item: SupervisorLiveEventPayload::ActionRequired { .. },
                    ..
                }
            )
        })
        .expect("expected action-required replay frame");
    let action_required_count = frames
        .iter()
        .filter(|frame| {
            matches!(
                frame,
                SupervisorControlResponse::Event {
                    item: SupervisorLiveEventPayload::ActionRequired { .. },
                    ..
                }
            )
        })
        .count();
    assert_eq!(action_required_count, 1);
    assert!(matches!(
        action_required_frame,
        SupervisorControlResponse::Event {
            item:
                SupervisorLiveEventPayload::ActionRequired {
                    action_id,
                    action_type,
                    title,
                    detail,
                    ..
                },
            ..
        } if action_id == &approval.action_id
            && action_type == "terminal_input_required"
            && title == "Terminal input required"
            && detail == "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run."
    ));

    let replay_dump = response_dump(&frames);
    assert!(!replay_dump.contains("access_token"));
    assert!(!replay_dump.contains("Paste access_token now"));

    let stopped = stop_runtime_run(&fresh_state, stop_request(project_id, &repo_root))
        .expect("stop interactive runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn detached_supervisor_persists_matching_autonomous_boundary_once_before_reload() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-interactive-autonomous";
    let repo_root = seed_project(&root, project_id, "repo-interactive-autonomous", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-interactive-autonomous",
            &runtime_shell::script_prompt_read_echo_and_sleep(
                "Enter deployment code: ",
                "value",
                "value=",
                5,
            ),
        ),
    )
    .expect("launch interactive runtime supervisor for autonomous persistence");
    seed_active_autonomous_run(&repo_root, project_id, &launched.run.run_id);

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, &launched.run.run_id, None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    let (approval_action_id, boundary_id) = frames
        .iter()
        .find_map(|frame| match frame {
            SupervisorControlResponse::Event {
                item:
                    SupervisorLiveEventPayload::ActionRequired {
                        action_id,
                        boundary_id,
                        ..
                    },
                ..
            } => Some((action_id.clone(), boundary_id.clone())),
            _ => None,
        })
        .expect("expected action-required replay frame for autonomous persistence test");

    persist_supervisor_event(
        &repo_root,
        project_id,
        &SupervisorLiveEventPayload::ActionRequired {
            action_id: approval_action_id.clone(),
            boundary_id: boundary_id.clone(),
            action_type: "terminal_input_required".into(),
            title: "Terminal input required".into(),
            detail: "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.".into(),
        },
    )
    .expect("persist autonomous boundary from supervisor event")
    .expect("autonomous boundary persistence should return a snapshot");

    let boundary_snapshot = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("load autonomous run after boundary persistence")
        .expect("autonomous run should exist after boundary persistence");
    assert_eq!(
        boundary_snapshot.run.status,
        project_store::AutonomousRunStatus::Paused
    );
    assert_eq!(
        boundary_snapshot
            .unit
            .as_ref()
            .map(|unit| unit.status.clone()),
        Some(project_store::AutonomousUnitStatus::Blocked)
    );
    assert_eq!(
        boundary_snapshot
            .attempt
            .as_ref()
            .map(|attempt| attempt.status.clone()),
        Some(project_store::AutonomousUnitStatus::Blocked)
    );

    let boundary_evidence = boundary_snapshot
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                        && payload.action_id.as_deref() == Some(approval_action_id.as_str())
                        && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(boundary_evidence.len(), 1);

    let approval_action_id = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load project snapshot after autonomous boundary persist")
        .snapshot
        .approval_requests[0]
        .action_id
        .clone();
    assert!(approval_action_id.contains(boundary_id.as_str()));

    let fresh_state = DesktopState::default();
    let recovered = probe_runtime_run(&fresh_state, probe_request(project_id, &repo_root))
        .expect("probe runtime run with fresh host state")
        .expect("runtime run should still exist after fresh probe");
    assert_eq!(recovered.run.run_id, launched.run.run_id);

    let replayed_snapshot = project_store::load_autonomous_run(&repo_root, project_id)
        .expect("reload autonomous run after fresh probe")
        .expect("autonomous run should still exist after fresh probe");
    let replayed_boundary_evidence = replayed_snapshot
        .history
        .iter()
        .flat_map(|entry| entry.artifacts.iter())
        .filter(|artifact| {
            matches!(
                artifact.payload.as_ref(),
                Some(project_store::AutonomousArtifactPayloadRecord::VerificationEvidence(payload))
                    if payload.boundary_id.as_deref() == Some(boundary_id.as_str())
                        && payload.outcome == project_store::AutonomousVerificationOutcomeRecord::Blocked
            )
        })
        .count();
    assert_eq!(replayed_boundary_evidence, 1);

    let stopped = stop_runtime_run(&fresh_state, stop_request(project_id, &repo_root))
        .expect("stop interactive runtime supervisor after autonomous persistence test")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn detached_supervisor_coalesces_repeated_prompt_churn_into_one_boundary() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-interactive-repeat";
    let repo_root = seed_project(&root, project_id, "repo-interactive-repeat", "repo");
    let state = DesktopState::default();

    launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-interactive-repeat",
            &runtime_shell::script_repeat_prompt_and_sleep("Enter deployment code: ", 2, 5),
        ),
    )
    .expect("launch repeated interactive runtime supervisor");

    let running = wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let action_required_count = running
        .checkpoints
        .iter()
        .filter(|checkpoint| {
            checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
        })
        .count();
    assert_eq!(action_required_count, 1);

    let project_snapshot = project_store::load_project_snapshot(&repo_root, project_id)
        .expect("load project snapshot")
        .snapshot;
    assert_eq!(project_snapshot.approval_requests.len(), 1);

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop repeated interactive runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn detached_supervisor_submit_input_routes_bytes_through_owned_writer() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-submit-success";
    let repo_root = seed_project(&root, project_id, "repo-submit-success", "repo");
    let state = DesktopState::default();

    let launched = launch_detached_runtime_supervisor(
        &state,
        launch_request(
            project_id,
            &repo_root,
            "run-submit-success",
            &runtime_shell::script_prompt_read_echo_and_sleep(
                "Enter value: ",
                "value",
                "value=",
                5,
            ),
        ),
    )
    .expect("launch interactive runtime supervisor");

    wait_for_runtime_run(&state, &repo_root, project_id, |snapshot| {
        snapshot.run.status == project_store::RuntimeRunStatus::Running
            && snapshot.checkpoints.iter().any(|checkpoint| {
                checkpoint.kind == project_store::RuntimeRunCheckpointKind::ActionRequired
            })
    });

    let mut reader = attach_reader(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::attach(project_id, "run-submit-success", None),
    );
    let attached = expect_attach_ack(read_supervisor_response(&mut reader));
    let frames = read_event_frames(&mut reader, attached.replayed_count);
    let (action_id, boundary_id) = match &frames[0] {
        SupervisorControlResponse::Event {
            item:
                SupervisorLiveEventPayload::ActionRequired {
                    action_id,
                    boundary_id,
                    ..
                },
            ..
        } => (action_id.clone(), boundary_id.clone()),
        other => panic!("expected action-required replay frame, got {other:?}"),
    };

    let submit = send_control_request(
        &launched.run.transport.endpoint,
        SupervisorControlRequest::submit_input(
            project_id,
            "run-submit-success",
            "session-1",
            Some("flow-1".into()),
            action_id,
            boundary_id,
            "approved",
        ),
    );
    assert!(matches!(
        submit,
        SupervisorControlResponse::SubmitInputAccepted { .. }
    ));

    let deadline = Instant::now() + Duration::from_secs(3);
    let mut saw_delivery = false;
    let mut saw_transcript = false;
    while Instant::now() < deadline && !(saw_delivery && saw_transcript) {
        match read_supervisor_response(&mut reader) {
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Activity { code, .. },
                ..
            } if code == "runtime_supervisor_input_delivered" => saw_delivery = true,
            SupervisorControlResponse::Event {
                item: SupervisorLiveEventPayload::Transcript { text },
                ..
            } if text == "value=approved" => saw_transcript = true,
            _ => {}
        }
    }

    assert!(saw_delivery, "expected input-delivered activity frame");
    assert!(saw_transcript, "expected resumed transcript output");

    let stopped = stop_runtime_run(&state, stop_request(project_id, &repo_root))
        .expect("stop submit-success runtime supervisor")
        .expect("runtime run should exist after stop");
    assert_eq!(stopped.run.status, project_store::RuntimeRunStatus::Stopped);
}

#[test]
fn submit_runtime_run_input_rejects_mismatched_ack_and_preserves_running_projection() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-submit-ack-mismatch";
    let repo_root = seed_project(&root, project_id, "repo-submit-ack-mismatch", "repo");
    let state = DesktopState::default();

    let mismatched_ack = SupervisorControlResponse::SubmitInputAccepted {
        protocol_version: SUPERVISOR_PROTOCOL_VERSION,
        project_id: project_id.into(),
        run_id: "run-submit-ack-mismatch".into(),
        action_id: "forged-action".into(),
        boundary_id: "boundary-1".into(),
        delivered_at: "2026-04-16T00:00:02Z".into(),
    };
    let (endpoint, server) = spawn_single_response_control_server(
        serde_json::to_string(&mismatched_ack).expect("serialize mismatched ack"),
    );
    seed_running_runtime_run(&repo_root, project_id, "run-submit-ack-mismatch", &endpoint);

    let error = submit_runtime_run_input(
        &state,
        RuntimeSupervisorSubmitInputRequest {
            project_id: project_id.into(),
            repo_root: repo_root.clone(),
            run_id: "run-submit-ack-mismatch".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            action_id: "expected-action".into(),
            boundary_id: "boundary-1".into(),
            input: "approved".into(),
            control_timeout: Duration::from_millis(750),
        },
    )
    .expect_err("submit should reject mismatched acknowledgement identity");
    assert_eq!(error.code, "runtime_supervisor_submit_ack_mismatch");

    server
        .join()
        .expect("mock mismatched-ack control server thread should complete");

    let snapshot = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load runtime run after mismatched ack")
        .expect("runtime run should still exist after mismatched ack");
    assert_eq!(
        snapshot.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert_eq!(
        snapshot.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Reachable
    );
    assert_eq!(
        snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_submit_ack_mismatch")
    );
}

#[test]
fn submit_runtime_run_input_preserves_running_projection_on_malformed_control_response() {
    let _guard = supervisor_test_guard();
    let root = tempfile::tempdir().expect("temp dir");
    let project_id = "project-submit-malformed-response";
    let repo_root = seed_project(&root, project_id, "repo-submit-malformed-response", "repo");
    let state = DesktopState::default();

    let (endpoint, server) = spawn_single_response_control_server("not-json".into());
    seed_running_runtime_run(
        &repo_root,
        project_id,
        "run-submit-malformed-response",
        &endpoint,
    );

    let error = submit_runtime_run_input(
        &state,
        RuntimeSupervisorSubmitInputRequest {
            project_id: project_id.into(),
            repo_root: repo_root.clone(),
            run_id: "run-submit-malformed-response".into(),
            session_id: "session-1".into(),
            flow_id: Some("flow-1".into()),
            action_id: "expected-action".into(),
            boundary_id: "boundary-1".into(),
            input: "approved".into(),
            control_timeout: Duration::from_millis(750),
        },
    )
    .expect_err("submit should fail closed when control response is malformed");
    assert_eq!(error.code, "runtime_supervisor_control_invalid");

    server
        .join()
        .expect("mock malformed-response control server thread should complete");

    let snapshot = project_store::load_runtime_run(&repo_root, project_id)
        .expect("load runtime run after malformed control response")
        .expect("runtime run should still exist after malformed control response");
    assert_eq!(
        snapshot.run.status,
        project_store::RuntimeRunStatus::Running
    );
    assert_eq!(
        snapshot.run.transport.liveness,
        project_store::RuntimeRunTransportLiveness::Reachable
    );
    assert_eq!(
        snapshot
            .run
            .last_error
            .as_ref()
            .map(|error| error.code.as_str()),
        Some("runtime_supervisor_control_invalid")
    );
}
