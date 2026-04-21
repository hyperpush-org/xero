pub(crate) use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Mutex, MutexGuard, OnceLock},
    thread,
    time::{Duration, Instant},
};

pub(crate) use cadence_desktop_lib::{
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
pub(crate) use tempfile::TempDir;

#[path = "../support/runtime_shell.rs"]
pub(crate) mod runtime_shell;

pub(crate) const STRUCTURED_EVENT_PREFIX: &str = "__Cadence_EVENT__ ";
pub(crate) const ATTACH_READ_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) fn supervisor_test_guard() -> MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) fn seed_project(root: &TempDir, project_id: &str, repository_id: &str, repo_name: &str) -> PathBuf {
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

pub(crate) fn supervisor_binary_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_Cadence-runtime-supervisor"))
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

pub(crate) fn probe_request(project_id: &str, repo_root: &Path) -> RuntimeSupervisorProbeRequest {
    RuntimeSupervisorProbeRequest {
        project_id: project_id.into(),
        repo_root: repo_root.to_path_buf(),
        control_timeout: Duration::from_millis(750),
    }
}

pub(crate) fn stop_request(project_id: &str, repo_root: &Path) -> RuntimeSupervisorStopRequest {
    RuntimeSupervisorStopRequest {
        project_id: project_id.into(),
        repo_root: repo_root.to_path_buf(),
        control_timeout: Duration::from_millis(750),
        shutdown_timeout: Duration::from_secs(4),
    }
}

pub(crate) fn seed_running_runtime_run(repo_root: &Path, project_id: &str, run_id: &str, endpoint: &str) {
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

pub(crate) fn seed_active_autonomous_run(repo_root: &Path, project_id: &str, run_id: &str) {
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

pub(crate) fn spawn_single_response_control_server(response_line: String) -> (String, thread::JoinHandle<()>) {
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

pub(crate) fn wait_for_runtime_run(
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

pub(crate) fn attach_reader(endpoint: &str, request: SupervisorControlRequest) -> BufReader<TcpStream> {
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

pub(crate) fn send_control_request(
    endpoint: &str,
    request: SupervisorControlRequest,
) -> SupervisorControlResponse {
    let mut reader = attach_reader(endpoint, request);
    read_supervisor_response(&mut reader)
}

pub(crate) fn read_supervisor_response(reader: &mut BufReader<TcpStream>) -> SupervisorControlResponse {
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .expect("read supervisor response");
    assert!(bytes > 0, "expected a supervisor response frame");
    serde_json::from_str(line.trim()).expect("decode supervisor response")
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct AttachAck {
    pub(crate) replayed_count: usize,
    pub(crate) replay_truncated: bool,
    pub(crate) oldest_available_sequence: Option<u64>,
    pub(crate) latest_sequence: Option<u64>,
}

pub(crate) fn expect_attach_ack(response: SupervisorControlResponse) -> AttachAck {
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

pub(crate) fn read_event_frames(
    reader: &mut BufReader<TcpStream>,
    replayed_count: usize,
) -> Vec<SupervisorControlResponse> {
    (0..replayed_count)
        .map(|_| read_supervisor_response(reader))
        .collect()
}

pub(crate) fn assert_monotonic_sequences(frames: &[SupervisorControlResponse], expected_run_id: &str) {
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

pub(crate) fn response_dump(frames: &[SupervisorControlResponse]) -> String {
    frames
        .iter()
        .map(|frame| serde_json::to_string(frame).expect("serialize frame"))
        .collect::<Vec<_>>()
        .join("\n")
}
