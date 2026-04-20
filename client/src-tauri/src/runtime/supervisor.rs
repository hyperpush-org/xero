use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{sync_channel, RecvTimeoutError, SyncSender, TrySendError},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, PtySize};
use serde::{de::DeserializeOwned, Serialize};

use crate::{
    auth::now_timestamp,
    commands::{validate_non_empty, CommandError},
    db::project_store::{
        self, NotificationDispatchEnqueueStatus, RuntimeActionRequiredUpsertRecord,
        RuntimeRunDiagnosticRecord, RuntimeRunRecord, RuntimeRunSnapshotRecord, RuntimeRunStatus,
        RuntimeRunTransportLiveness, RuntimeRunTransportRecord, RuntimeRunUpsertRecord,
    },
    state::DesktopState,
};

use super::{
    platform_adapter::resolve_runtime_supervisor_binary,
    protocol::{
        CommandToolResultSummary, FileToolResultSummary, GitToolResultSummary,
        SupervisorControlRequest, SupervisorControlResponse, SupervisorLiveEventPayload,
        SupervisorProcessStatus, SupervisorProtocolDiagnostic, SupervisorStartupMessage,
        SupervisorToolCallState, ToolResultSummary, WebToolResultSummary,
        SUPERVISOR_KIND_DETACHED_PTY, SUPERVISOR_PROTOCOL_VERSION, SUPERVISOR_TRANSPORT_KIND_TCP,
    },
};

const DEFAULT_STARTUP_TIMEOUT: Duration = Duration::from_secs(5);
const DEFAULT_CONTROL_TIMEOUT: Duration = Duration::from_millis(750);
const DEFAULT_STOP_TIMEOUT: Duration = Duration::from_secs(3);
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(1);
const CONTROL_ACCEPT_POLL_INTERVAL: Duration = Duration::from_millis(25);
const TERMINAL_ATTACH_GRACE_PERIOD: Duration = Duration::from_secs(1);
const PROTOCOL_LINE_LIMIT: usize = 16 * 1024;
const LIVE_EVENT_RING_LIMIT: usize = 128;
const LIVE_EVENT_SUBSCRIBER_BUFFER: usize = 32;
const MAX_LIVE_EVENT_FRAGMENT_BYTES: usize = 4096;
const MAX_LIVE_EVENT_TEXT_CHARS: usize = 512;
const MAX_CONTROL_INPUT_CHARS: usize = 4096;
const STRUCTURED_EVENT_PREFIX: &str = "__CADENCE_EVENT__ ";
const SHELL_OUTPUT_PREFIX: &str = "Shell output:";
const ACTIVITY_OUTPUT_PREFIX: &str = "Supervisor activity:";
const INTERACTIVE_BOUNDARY_ACTION_TYPE: &str = "terminal_input_required";
const INTERACTIVE_BOUNDARY_TITLE: &str = "Terminal input required";
const INTERACTIVE_BOUNDARY_DETAIL: &str = "Detached runtime is blocked on terminal input. Approve and resume with a coarse operator answer to continue the same supervised run.";
const INTERACTIVE_BOUNDARY_CHECKPOINT_SUMMARY: &str =
    "Detached runtime blocked on terminal input and is awaiting operator approval.";
const REDACTED_SHELL_OUTPUT_SUMMARY: &str = "Shell output was redacted before durable persistence.";
const REDACTED_LIVE_EVENT_DETAIL: &str =
    "Cadence redacted secret-bearing live output before replay and persistence.";

#[derive(Debug, Clone, Default)]
pub struct RuntimeSupervisorController {
    inner: Arc<Mutex<RuntimeSupervisorRegistry>>,
}

#[derive(Debug, Default)]
struct RuntimeSupervisorRegistry {
    active: HashMap<String, ActiveRuntimeSupervisor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ActiveRuntimeSupervisor {
    project_id: String,
    run_id: String,
    endpoint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveRuntimeSupervisorSnapshot {
    pub project_id: String,
    pub run_id: String,
    pub endpoint: String,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorLaunchRequest {
    pub project_id: String,
    pub repo_root: PathBuf,
    pub runtime_kind: String,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub program: String,
    pub args: Vec<String>,
    pub startup_timeout: Duration,
    pub control_timeout: Duration,
    pub supervisor_binary: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorProbeRequest {
    pub project_id: String,
    pub repo_root: PathBuf,
    pub control_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorStopRequest {
    pub project_id: String,
    pub repo_root: PathBuf,
    pub control_timeout: Duration,
    pub shutdown_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct RuntimeSupervisorSubmitInputRequest {
    pub project_id: String,
    pub repo_root: PathBuf,
    pub run_id: String,
    pub session_id: String,
    pub flow_id: Option<String>,
    pub action_id: String,
    pub boundary_id: String,
    pub input: String,
    pub control_timeout: Duration,
}

#[derive(Debug, Clone)]
struct RuntimeSupervisorSidecarArgs {
    project_id: String,
    repo_root: PathBuf,
    runtime_kind: String,
    run_id: String,
    session_id: String,
    flow_id: Option<String>,
    program: String,
    args: Vec<String>,
}

#[derive(Debug, Clone)]
struct ActiveInteractiveBoundary {
    boundary_id: String,
    action_id: String,
    action_type: String,
    title: String,
    detail: String,
    detected_at: String,
}

#[derive(Debug, Clone)]
struct SidecarSharedState {
    project_id: String,
    run_id: String,
    runtime_kind: String,
    session_id: String,
    flow_id: Option<String>,
    endpoint: String,
    started_at: String,
    child_pid: Option<u32>,
    status: SupervisorProcessStatus,
    stop_requested: bool,
    last_heartbeat_at: Option<String>,
    last_checkpoint_sequence: u32,
    last_checkpoint_at: Option<String>,
    last_error: Option<SupervisorProtocolDiagnostic>,
    stopped_at: Option<String>,
    next_boundary_serial: u64,
    active_boundary: Option<ActiveInteractiveBoundary>,
}

#[derive(Debug, Clone)]
struct BufferedSupervisorEvent {
    project_id: String,
    run_id: String,
    sequence: u64,
    created_at: String,
    item: SupervisorLiveEventPayload,
}

#[derive(Debug, Default)]
struct SupervisorEventHub {
    next_sequence: u64,
    next_subscriber_id: u64,
    ring: VecDeque<BufferedSupervisorEvent>,
    subscribers: HashMap<u64, SyncSender<BufferedSupervisorEvent>>,
}

#[derive(Debug, Default)]
struct PtyEventNormalizer {
    pending: Vec<u8>,
}

type SharedPtyWriter = Arc<Mutex<Box<dyn Write + Send>>>;

#[derive(Debug, Clone)]
struct InteractiveBoundaryCandidate {
    action_type: String,
    title: String,
    detail: String,
    checkpoint_summary: String,
}

#[derive(Debug, Clone)]
struct NormalizedPtyEvent {
    item: SupervisorLiveEventPayload,
    checkpoint_summary: Option<String>,
}

#[derive(Debug, Clone)]
struct ReplayRegistration {
    subscriber_id: u64,
    attach_response: SupervisorControlResponse,
    replay_events: Vec<BufferedSupervisorEvent>,
}

impl Default for RuntimeSupervisorLaunchRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            repo_root: PathBuf::new(),
            runtime_kind: "openai_codex".into(),
            run_id: String::new(),
            session_id: String::new(),
            flow_id: None,
            program: String::new(),
            args: Vec::new(),
            startup_timeout: DEFAULT_STARTUP_TIMEOUT,
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
            supervisor_binary: None,
        }
    }
}

impl Default for RuntimeSupervisorProbeRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            repo_root: PathBuf::new(),
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
        }
    }
}

impl Default for RuntimeSupervisorStopRequest {
    fn default() -> Self {
        Self {
            project_id: String::new(),
            repo_root: PathBuf::new(),
            control_timeout: DEFAULT_CONTROL_TIMEOUT,
            shutdown_timeout: DEFAULT_STOP_TIMEOUT,
        }
    }
}

impl RuntimeSupervisorController {
    pub fn remember(&self, project_id: &str, run_id: &str, endpoint: &str) {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .active
            .insert(
                project_id.into(),
                ActiveRuntimeSupervisor {
                    project_id: project_id.into(),
                    run_id: run_id.into(),
                    endpoint: endpoint.into(),
                },
            );
    }

    pub fn forget(&self, project_id: &str) {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .active
            .remove(project_id);
    }

    pub fn snapshot(&self, project_id: &str) -> Option<ActiveRuntimeSupervisorSnapshot> {
        self.inner
            .lock()
            .expect("runtime supervisor registry poisoned")
            .active
            .get(project_id)
            .cloned()
            .map(|entry| ActiveRuntimeSupervisorSnapshot {
                project_id: entry.project_id,
                run_id: entry.run_id,
                endpoint: entry.endpoint,
            })
    }
}

pub fn launch_detached_runtime_supervisor(
    state: &DesktopState,
    request: RuntimeSupervisorLaunchRequest,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    validate_launch_request(&request)?;

    if let Some(existing) = probe_runtime_run_with_timeout(
        state,
        &request.repo_root,
        &request.project_id,
        request.control_timeout,
    )? {
        if matches!(
            existing.run.status,
            RuntimeRunStatus::Starting | RuntimeRunStatus::Running
        ) && existing.run.transport.liveness == RuntimeRunTransportLiveness::Reachable
        {
            return Err(CommandError::user_fixable(
                "runtime_run_already_active",
                format!(
                    "Cadence already has a detached runtime supervisor for project `{}`. Stop or reconnect to run `{}` before starting another one.",
                    request.project_id, existing.run.run_id
                ),
            ));
        }
    }

    let supervisor_binary_resolution =
        resolve_runtime_supervisor_binary(request.supervisor_binary.as_deref())?;
    let supervisor_binary = supervisor_binary_resolution.binary_path;

    let mut sidecar = Command::new(&supervisor_binary);
    sidecar
        .arg("--project-id")
        .arg(&request.project_id)
        .arg("--repo-root")
        .arg(&request.repo_root)
        .arg("--runtime-kind")
        .arg(&request.runtime_kind)
        .arg("--run-id")
        .arg(&request.run_id)
        .arg("--session-id")
        .arg(&request.session_id)
        .arg("--program")
        .arg(&request.program)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());

    if let Some(flow_id) = request.flow_id.as_deref() {
        sidecar.arg("--flow-id").arg(flow_id);
    }

    for arg in &request.args {
        sidecar.arg("--command-arg").arg(arg);
    }

    let mut child = sidecar.spawn().map_err(|error| {
        let _ = persist_failed_launch(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
            &request.runtime_kind,
            "runtime_supervisor_spawn_failed",
            "Cadence could not launch the detached PTY supervisor process.",
        );
        CommandError::retryable(
            "runtime_supervisor_spawn_failed",
            format!(
                "Cadence could not launch the detached PTY supervisor process from `{}`: {error}",
                supervisor_binary.display()
            ),
        )
    })?;

    let stdout = child.stdout.take().ok_or_else(|| {
        let _ = child.kill();
        let _ = persist_failed_launch(
            &request.repo_root,
            &request.project_id,
            &request.run_id,
            &request.runtime_kind,
            "runtime_supervisor_stdout_missing",
            "Cadence could not read the detached PTY supervisor startup handshake.",
        );
        CommandError::system_fault(
            "runtime_supervisor_stdout_missing",
            "Cadence launched the detached PTY supervisor but could not read its startup handshake.",
        )
    })?;

    let startup_message = match read_startup_message(stdout, request.startup_timeout) {
        Ok(message) => message,
        Err(error) => {
            let _ = child.kill();
            let _ = persist_failed_launch(
                &request.repo_root,
                &request.project_id,
                &request.run_id,
                &request.runtime_kind,
                &error.code,
                &error.message,
            );
            return Err(error);
        }
    };

    match startup_message {
        SupervisorStartupMessage::Ready {
            protocol_version,
            project_id,
            run_id,
            transport_kind,
            endpoint,
            status,
            ..
        } => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                let _ = child.kill();
                let _ = persist_failed_launch(
                    &request.repo_root,
                    &request.project_id,
                    &request.run_id,
                    &request.runtime_kind,
                    "runtime_supervisor_protocol_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because its protocol version was unsupported.",
                );
                return Err(CommandError::retryable(
                    "runtime_supervisor_protocol_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because its protocol version was unsupported.",
                ));
            }

            if project_id != request.project_id || run_id != request.run_id {
                let _ = child.kill();
                let _ = persist_failed_launch(
                    &request.repo_root,
                    &request.project_id,
                    &request.run_id,
                    &request.runtime_kind,
                    "runtime_supervisor_handshake_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because it did not match the requested project or run id.",
                );
                return Err(CommandError::retryable(
                    "runtime_supervisor_handshake_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because it did not match the requested project or run id.",
                ));
            }

            if transport_kind != SUPERVISOR_TRANSPORT_KIND_TCP || endpoint.trim().is_empty() {
                let _ = child.kill();
                let _ = persist_failed_launch(
                    &request.repo_root,
                    &request.project_id,
                    &request.run_id,
                    &request.runtime_kind,
                    "runtime_supervisor_handshake_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because it omitted a valid control endpoint.",
                );
                return Err(CommandError::retryable(
                    "runtime_supervisor_handshake_invalid",
                    "Cadence rejected the detached PTY supervisor handshake because it omitted a valid control endpoint.",
                ));
            }

            let expected_status = match status {
                SupervisorProcessStatus::Starting => RuntimeRunStatus::Starting,
                SupervisorProcessStatus::Running => RuntimeRunStatus::Running,
                SupervisorProcessStatus::Stopped => RuntimeRunStatus::Stopped,
                SupervisorProcessStatus::Failed => RuntimeRunStatus::Failed,
            };

            let snapshot =
                project_store::load_runtime_run(&request.repo_root, &request.project_id)?
                    .filter(|snapshot| snapshot.run.run_id == request.run_id)
                    .map(Ok)
                    .unwrap_or_else(|| {
                        project_store::upsert_runtime_run(
                            &request.repo_root,
                            &RuntimeRunUpsertRecord {
                                run: RuntimeRunRecord {
                                    project_id: request.project_id.clone(),
                                    run_id: request.run_id.clone(),
                                    runtime_kind: request.runtime_kind.clone(),
                                    supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                                    status: expected_status,
                                    transport: RuntimeRunTransportRecord {
                                        kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                                        endpoint: endpoint.clone(),
                                        liveness: RuntimeRunTransportLiveness::Reachable,
                                    },
                                    started_at: now_timestamp(),
                                    last_heartbeat_at: Some(now_timestamp()),
                                    stopped_at: None,
                                    last_error: None,
                                    updated_at: now_timestamp(),
                                },
                                checkpoint: None,
                            },
                        )
                    })?;

            state.runtime_supervisor_controller().remember(
                &request.project_id,
                &request.run_id,
                &endpoint,
            );
            Ok(snapshot)
        }
        SupervisorStartupMessage::Error {
            code,
            message,
            retryable,
            ..
        } => {
            let _ = persist_failed_launch(
                &request.repo_root,
                &request.project_id,
                &request.run_id,
                &request.runtime_kind,
                &code,
                &message,
            );
            if retryable {
                Err(CommandError::retryable(code, message))
            } else {
                Err(CommandError::user_fixable(code, message))
            }
        }
    }
}

pub fn probe_runtime_run(
    state: &DesktopState,
    request: RuntimeSupervisorProbeRequest,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;
    probe_runtime_run_with_timeout(
        state,
        &request.repo_root,
        &request.project_id,
        request.control_timeout,
    )
}

pub fn stop_runtime_run(
    state: &DesktopState,
    request: RuntimeSupervisorStopRequest,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;

    let Some(snapshot) = project_store::load_runtime_run(&request.repo_root, &request.project_id)?
    else {
        state
            .runtime_supervisor_controller()
            .forget(&request.project_id);
        return Ok(None);
    };

    if matches!(
        snapshot.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    ) {
        state
            .runtime_supervisor_controller()
            .forget(&request.project_id);
        return Ok(Some(snapshot));
    }

    let response = send_control_request(
        &snapshot.run.transport.endpoint,
        request.control_timeout,
        &SupervisorControlRequest::stop(&snapshot.run.project_id, &snapshot.run.run_id),
    );

    match response {
        Ok(SupervisorControlResponse::StopAccepted {
            project_id, run_id, ..
        }) if project_id == snapshot.run.project_id && run_id == snapshot.run.run_id => {}
        Ok(_) => {
            return mark_runtime_run_after_probe_failure(
                state,
                &request.repo_root,
                snapshot,
                "supervisor_stop_invalid",
                "The detached supervisor returned a mismatched stop acknowledgement.",
            )
            .map(Some)
        }
        Err(_) => {
            return mark_runtime_run_after_probe_failure(
                state,
                &request.repo_root,
                snapshot,
                "supervisor_stop_failed",
                "The detached supervisor did not accept the stop request.",
            )
            .map(Some)
        }
    }

    let deadline = std::time::Instant::now() + request.shutdown_timeout;
    loop {
        let latest = project_store::load_runtime_run(&request.repo_root, &request.project_id)?;
        if let Some(snapshot) = latest {
            if matches!(
                snapshot.run.status,
                RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
            ) {
                state
                    .runtime_supervisor_controller()
                    .forget(&request.project_id);
                return Ok(Some(snapshot));
            }
        }

        if std::time::Instant::now() >= deadline {
            let Some(latest) =
                project_store::load_runtime_run(&request.repo_root, &request.project_id)?
            else {
                state
                    .runtime_supervisor_controller()
                    .forget(&request.project_id);
                return Ok(None);
            };
            let latest = mark_runtime_run_after_probe_failure(
                state,
                &request.repo_root,
                latest,
                "supervisor_stop_timeout",
                "The detached supervisor did not stop before the shutdown timeout elapsed.",
            )?;
            return Ok(Some(latest));
        }

        match send_control_request(
            &snapshot.run.transport.endpoint,
            request.control_timeout,
            &SupervisorControlRequest::probe(&snapshot.run.project_id, &snapshot.run.run_id),
        ) {
            Ok(SupervisorControlResponse::ProbeResult { .. }) => {}
            Ok(_) | Err(_) => {}
        }

        thread::sleep(Duration::from_millis(100));
    }
}

pub fn submit_runtime_run_input(
    state: &DesktopState,
    request: RuntimeSupervisorSubmitInputRequest,
) -> Result<String, CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.session_id, "sessionId")?;
    validate_non_empty(&request.action_id, "actionId")?;
    validate_non_empty(&request.boundary_id, "boundaryId")?;
    validate_non_empty(&request.input, "input")?;
    if let Some(flow_id) = request.flow_id.as_deref() {
        validate_non_empty(flow_id, "flowId")?;
    }
    if request.control_timeout.is_zero() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_request_invalid",
            "Cadence requires a non-zero detached supervisor control timeout.",
        ));
    }

    let Some(snapshot) = project_store::load_runtime_run(&request.repo_root, &request.project_id)?
    else {
        state
            .runtime_supervisor_controller()
            .forget(&request.project_id);
        return Err(CommandError::retryable(
            "runtime_run_missing",
            format!(
                "Cadence cannot deliver approved terminal input for action `{}` because project `{}` has no durable runtime run.",
                request.action_id, request.project_id
            ),
        ));
    };

    if snapshot.run.run_id != request.run_id {
        return Err(CommandError::retryable(
            "runtime_run_mismatch",
            format!(
                "Cadence refused to deliver approved terminal input for action `{}` because project `{}` is currently bound to durable run `{}` instead of `{}`.",
                request.action_id, request.project_id, snapshot.run.run_id, request.run_id
            ),
        ));
    }

    if let Some(active) = state
        .runtime_supervisor_controller()
        .snapshot(&request.project_id)
        .filter(|active| active.run_id != request.run_id)
    {
        return Err(CommandError::retryable(
            "runtime_run_mismatch",
            format!(
                "Cadence refused to deliver approved terminal input for action `{}` because project `{}` is currently attached to run `{}` instead of `{}`.",
                request.action_id, request.project_id, active.run_id, request.run_id
            ),
        ));
    }

    if matches!(
        snapshot.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    ) {
        return Err(CommandError::retryable(
            "runtime_run_unavailable",
            format!(
                "Cadence cannot deliver approved terminal input for action `{}` because run `{}` is already terminal ({}). Refresh runtime state before retrying.",
                request.action_id,
                request.run_id,
                runtime_run_status_label(&snapshot.run.status)
            ),
        ));
    }

    let response = send_control_request(
        &snapshot.run.transport.endpoint,
        request.control_timeout,
        &SupervisorControlRequest::submit_input(
            &request.project_id,
            &request.run_id,
            &request.session_id,
            request.flow_id.clone(),
            &request.action_id,
            &request.boundary_id,
            &request.input,
        ),
    );

    let persist_control_error = |code: &str, message: &str| -> Result<(), CommandError> {
        refresh_runtime_run_after_control_response(
            &request.repo_root,
            &snapshot,
            Some(RuntimeRunDiagnosticRecord {
                code: code.to_string(),
                message: message.to_string(),
            }),
        )?;
        Ok(())
    };

    match response {
        Ok(SupervisorControlResponse::SubmitInputAccepted {
            protocol_version,
            project_id,
            run_id,
            action_id,
            boundary_id,
            delivered_at,
        }) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                let code = "runtime_supervisor_protocol_invalid";
                let message =
                    "Cadence rejected a detached supervisor submit-input acknowledgement with an unexpected protocol version.";
                persist_control_error(code, message)?;
                return Err(CommandError::user_fixable(code, message));
            }

            if project_id != snapshot.run.project_id
                || run_id != snapshot.run.run_id
                || action_id != request.action_id
                || boundary_id != request.boundary_id
            {
                let code = "runtime_supervisor_submit_ack_mismatch";
                let message =
                    "Cadence rejected detached supervisor submit-input acknowledgement because its runtime identity did not match the approved action boundary.";
                persist_control_error(code, message)?;
                return Err(CommandError::retryable(code, message));
            }

            let refreshed =
                refresh_runtime_run_after_control_response(&request.repo_root, &snapshot, None)?;
            state.runtime_supervisor_controller().remember(
                &refreshed.run.project_id,
                &refreshed.run.run_id,
                &refreshed.run.transport.endpoint,
            );
            Ok(delivered_at)
        }
        Ok(SupervisorControlResponse::Error {
            protocol_version,
            code,
            message,
            retryable,
        }) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                let code = "runtime_supervisor_protocol_invalid";
                let message =
                    "Cadence rejected a detached supervisor control error frame with an unexpected protocol version.";
                persist_control_error(code, message)?;
                return Err(CommandError::user_fixable(code, message));
            }

            persist_control_error(&code, &message)?;
            Err(if retryable {
                CommandError::retryable(code, message)
            } else {
                CommandError::user_fixable(code, message)
            })
        }
        Ok(_) => {
            let code = "runtime_supervisor_control_invalid";
            let message =
                "Cadence rejected an unsupported detached supervisor submit-input acknowledgement.";
            persist_control_error(code, message)?;
            Err(CommandError::retryable(code, message))
        }
        Err(error) => {
            if error.code == "runtime_supervisor_control_invalid" {
                persist_control_error(&error.code, &error.message)?;
                return Err(error);
            }

            upsert_runtime_run_projection(
                &request.repo_root,
                &snapshot,
                RuntimeRunStatus::Stale,
                RuntimeRunTransportLiveness::Unreachable,
                Some(RuntimeRunDiagnosticRecord {
                    code: error.code.clone(),
                    message: error.message.clone(),
                }),
                snapshot.run.last_heartbeat_at.clone(),
                snapshot.run.stopped_at.clone(),
            )?;
            state
                .runtime_supervisor_controller()
                .forget(&request.project_id);
            Err(error)
        }
    }
}

pub fn run_supervisor_sidecar_from_env() -> Result<(), CommandError> {
    let args = parse_sidecar_args(std::env::args().skip(1))?;
    run_supervisor_sidecar(args)
}

fn validate_launch_request(request: &RuntimeSupervisorLaunchRequest) -> Result<(), CommandError> {
    validate_non_empty(&request.project_id, "projectId")?;
    validate_non_empty(&request.runtime_kind, "runtimeKind")?;
    validate_non_empty(&request.run_id, "runId")?;
    validate_non_empty(&request.session_id, "sessionId")?;
    if let Some(flow_id) = request.flow_id.as_deref() {
        validate_non_empty(flow_id, "flowId")?;
    }
    validate_non_empty(&request.program, "program")?;

    if request.startup_timeout.is_zero() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_request_invalid",
            "Cadence requires a non-zero detached supervisor startup timeout.",
        ));
    }

    if request.control_timeout.is_zero() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_request_invalid",
            "Cadence requires a non-zero detached supervisor control timeout.",
        ));
    }

    Ok(())
}

fn probe_runtime_run_with_timeout(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    control_timeout: Duration,
) -> Result<Option<RuntimeRunSnapshotRecord>, CommandError> {
    let Some(snapshot) = project_store::load_runtime_run(repo_root, project_id)? else {
        state.runtime_supervisor_controller().forget(project_id);
        return Ok(None);
    };

    if matches!(
        snapshot.run.status,
        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
    ) {
        state.runtime_supervisor_controller().forget(project_id);
        return Ok(Some(snapshot));
    }

    match send_control_request(
        &snapshot.run.transport.endpoint,
        control_timeout,
        &SupervisorControlRequest::probe(&snapshot.run.project_id, &snapshot.run.run_id),
    ) {
        Ok(SupervisorControlResponse::ProbeResult {
            protocol_version,
            project_id,
            run_id,
            status,
            last_heartbeat_at,
            last_error,
            ..
        }) => {
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION
                || project_id != snapshot.run.project_id
                || run_id != snapshot.run.run_id
            {
                let updated = mark_runtime_run_after_probe_failure(
                    state,
                    repo_root,
                    snapshot,
                    "supervisor_probe_invalid",
                    "The detached supervisor returned a mismatched probe response.",
                )?;
                return Ok(Some(updated));
            }

            let Some(latest) = project_store::load_runtime_run(repo_root, project_id.as_str())?
            else {
                let updated = mark_runtime_run_after_probe_failure(
                    state,
                    repo_root,
                    snapshot,
                    "supervisor_probe_missing",
                    "Cadence could not reload the durable runtime-run row after a successful probe.",
                )?;
                return Ok(Some(updated));
            };

            let mapped_status = match status {
                SupervisorProcessStatus::Starting => RuntimeRunStatus::Starting,
                SupervisorProcessStatus::Running => RuntimeRunStatus::Running,
                SupervisorProcessStatus::Stopped => RuntimeRunStatus::Stopped,
                SupervisorProcessStatus::Failed => RuntimeRunStatus::Failed,
            };
            let response_error = last_error.map(protocol_diagnostic_into_record);
            let persisted_error = response_error
                .clone()
                .or_else(|| latest.run.last_error.clone());

            if latest.run.transport.liveness == RuntimeRunTransportLiveness::Reachable
                && latest.run.status == mapped_status
                && latest.run.last_error == persisted_error
            {
                if matches!(
                    latest.run.status,
                    RuntimeRunStatus::Starting | RuntimeRunStatus::Running
                ) {
                    state.runtime_supervisor_controller().remember(
                        &latest.run.project_id,
                        &latest.run.run_id,
                        &latest.run.transport.endpoint,
                    );
                } else {
                    state
                        .runtime_supervisor_controller()
                        .forget(&latest.run.project_id);
                }
                return Ok(Some(latest));
            }

            let updated = upsert_runtime_run_projection(
                repo_root,
                &latest,
                mapped_status,
                RuntimeRunTransportLiveness::Reachable,
                persisted_error,
                last_heartbeat_at,
                latest.run.stopped_at.clone(),
            )?;

            if matches!(
                updated.run.status,
                RuntimeRunStatus::Starting | RuntimeRunStatus::Running
            ) {
                state.runtime_supervisor_controller().remember(
                    &updated.run.project_id,
                    &updated.run.run_id,
                    &updated.run.transport.endpoint,
                );
            } else {
                state
                    .runtime_supervisor_controller()
                    .forget(&updated.run.project_id);
            }

            Ok(Some(updated))
        }
        Ok(SupervisorControlResponse::Error { code, message, .. }) => {
            if let Some(latest) = project_store::load_runtime_run(repo_root, project_id)? {
                if latest.run.run_id == snapshot.run.run_id
                    && matches!(
                        latest.run.status,
                        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
                    )
                {
                    state
                        .runtime_supervisor_controller()
                        .forget(&latest.run.project_id);
                    return Ok(Some(latest));
                }
            }

            let updated =
                mark_runtime_run_after_probe_failure(state, repo_root, snapshot, &code, &message)?;
            Ok(Some(updated))
        }
        Ok(_) => {
            if let Some(latest) = project_store::load_runtime_run(repo_root, project_id)? {
                if latest.run.run_id == snapshot.run.run_id
                    && matches!(
                        latest.run.status,
                        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
                    )
                {
                    state
                        .runtime_supervisor_controller()
                        .forget(&latest.run.project_id);
                    return Ok(Some(latest));
                }
            }

            let updated = mark_runtime_run_after_probe_failure(
                state,
                repo_root,
                snapshot,
                "supervisor_probe_invalid",
                "Cadence rejected the detached supervisor probe response because it did not match the probe contract.",
            )?;
            Ok(Some(updated))
        }
        Err(error) => {
            if let Some(latest) = project_store::load_runtime_run(repo_root, project_id)? {
                if latest.run.run_id == snapshot.run.run_id
                    && matches!(
                        latest.run.status,
                        RuntimeRunStatus::Stopped | RuntimeRunStatus::Failed
                    )
                {
                    state
                        .runtime_supervisor_controller()
                        .forget(&latest.run.project_id);
                    return Ok(Some(latest));
                }
            }

            let updated = mark_runtime_run_after_probe_failure(
                state,
                repo_root,
                snapshot,
                &error.code,
                &error.message,
            )?;
            Ok(Some(updated))
        }
    }
}

fn mark_runtime_run_after_probe_failure(
    state: &DesktopState,
    repo_root: &Path,
    snapshot: RuntimeRunSnapshotRecord,
    code: &str,
    message: &str,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    let updated = upsert_runtime_run_projection(
        repo_root,
        &snapshot,
        RuntimeRunStatus::Stale,
        RuntimeRunTransportLiveness::Unreachable,
        Some(RuntimeRunDiagnosticRecord {
            code: code.into(),
            message: message.into(),
        }),
        snapshot.run.last_heartbeat_at.clone(),
        snapshot.run.stopped_at.clone(),
    )?;
    state
        .runtime_supervisor_controller()
        .forget(&updated.run.project_id);
    Ok(updated)
}

fn refresh_runtime_run_after_control_response(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    last_error: Option<RuntimeRunDiagnosticRecord>,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    upsert_runtime_run_projection(
        repo_root,
        snapshot,
        RuntimeRunStatus::Running,
        RuntimeRunTransportLiveness::Reachable,
        last_error,
        snapshot.run.last_heartbeat_at.clone(),
        None,
    )
}

fn upsert_runtime_run_projection(
    repo_root: &Path,
    snapshot: &RuntimeRunSnapshotRecord,
    status: RuntimeRunStatus,
    liveness: RuntimeRunTransportLiveness,
    last_error: Option<RuntimeRunDiagnosticRecord>,
    last_heartbeat_at: Option<String>,
    stopped_at: Option<String>,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    if snapshot.run.status == status
        && snapshot.run.transport.liveness == liveness
        && snapshot.run.last_error.as_ref() == last_error.as_ref()
        && snapshot.run.last_heartbeat_at.as_deref() == last_heartbeat_at.as_deref()
        && snapshot.run.stopped_at.as_deref() == stopped_at.as_deref()
    {
        return Ok(snapshot.clone());
    }

    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: snapshot.run.project_id.clone(),
                run_id: snapshot.run.run_id.clone(),
                runtime_kind: snapshot.run.runtime_kind.clone(),
                supervisor_kind: snapshot.run.supervisor_kind.clone(),
                status,
                transport: RuntimeRunTransportRecord {
                    kind: snapshot.run.transport.kind.clone(),
                    endpoint: snapshot.run.transport.endpoint.clone(),
                    liveness,
                },
                started_at: snapshot.run.started_at.clone(),
                last_heartbeat_at,
                stopped_at,
                last_error,
                updated_at: now_timestamp(),
            },
            checkpoint: None,
        },
    )
}

fn runtime_run_status_label(status: &RuntimeRunStatus) -> &'static str {
    match status {
        RuntimeRunStatus::Starting => "starting",
        RuntimeRunStatus::Running => "running",
        RuntimeRunStatus::Stale => "stale",
        RuntimeRunStatus::Stopped => "stopped",
        RuntimeRunStatus::Failed => "failed",
    }
}

fn persist_failed_launch(
    repo_root: &Path,
    project_id: &str,
    run_id: &str,
    runtime_kind: &str,
    code: &str,
    message: &str,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: project_id.into(),
                run_id: run_id.into(),
                runtime_kind: runtime_kind.into(),
                supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                status: RuntimeRunStatus::Failed,
                transport: RuntimeRunTransportRecord {
                    kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                    endpoint: "launch-pending".into(),
                    liveness: RuntimeRunTransportLiveness::Unknown,
                },
                started_at: now_timestamp(),
                last_heartbeat_at: None,
                stopped_at: Some(now_timestamp()),
                last_error: Some(RuntimeRunDiagnosticRecord {
                    code: code.into(),
                    message: message.into(),
                }),
                updated_at: now_timestamp(),
            },
            checkpoint: None,
        },
    )
}

fn read_startup_message(
    stdout: impl Read + Send + 'static,
    timeout: Duration,
) -> Result<SupervisorStartupMessage, CommandError> {
    let (sender, receiver) = sync_channel(1);
    thread::spawn(move || {
        let result = read_json_line_from_reader::<_, SupervisorStartupMessage>(stdout).map_err(
            |error| {
                CommandError::retryable(
                    "runtime_supervisor_handshake_invalid",
                    format!(
                        "Cadence could not decode the detached PTY supervisor startup handshake: {error}"
                    ),
                )
            },
        );
        let _ = sender.send(result);
    });

    match receiver.recv_timeout(timeout) {
        Ok(result) => result,
        Err(RecvTimeoutError::Timeout) => Err(CommandError::retryable(
            "runtime_supervisor_start_timeout",
            "Cadence timed out while waiting for the detached PTY supervisor startup handshake.",
        )),
        Err(RecvTimeoutError::Disconnected) => Err(CommandError::retryable(
            "runtime_supervisor_handshake_invalid",
            "Cadence lost the detached PTY supervisor startup handshake before it completed.",
        )),
    }
}

fn send_control_request(
    endpoint: &str,
    timeout: Duration,
    request: &SupervisorControlRequest,
) -> Result<SupervisorControlResponse, CommandError> {
    let address = endpoint.parse::<SocketAddr>().map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_endpoint_invalid",
            "Cadence could not parse the detached supervisor control endpoint.",
        )
    })?;

    let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_connect_failed",
            "Cadence could not connect to the detached supervisor control endpoint.",
        )
    })?;
    stream.set_read_timeout(Some(timeout)).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_timeout_config_failed",
            "Cadence could not configure the detached supervisor control read timeout.",
        )
    })?;
    stream.set_write_timeout(Some(timeout)).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_timeout_config_failed",
            "Cadence could not configure the detached supervisor control write timeout.",
        )
    })?;

    write_json_line(&mut stream, request).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_write_failed",
            "Cadence could not write the detached supervisor control request.",
        )
    })?;

    read_json_line_from_reader::<_, SupervisorControlResponse>(stream).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_invalid",
            "Cadence could not decode the detached supervisor control response.",
        )
    })
}

fn write_json_line<W: Write, T: Serialize>(
    writer: &mut W,
    value: &T,
) -> Result<(), std::io::Error> {
    serde_json::to_writer(&mut *writer, value).map_err(std::io::Error::other)?;
    writer.write_all(b"\n")?;
    writer.flush()
}

fn read_json_line_from_reader<R: Read, T: DeserializeOwned>(reader: R) -> Result<T, String> {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    let bytes_read = reader
        .read_line(&mut line)
        .map_err(|error| format!("read failed: {error}"))?;

    if bytes_read == 0 {
        return Err("empty stream".into());
    }

    if line.len() > PROTOCOL_LINE_LIMIT {
        return Err("line exceeded protocol limit".into());
    }

    serde_json::from_str::<T>(line.trim()).map_err(|error| format!("json decode failed: {error}"))
}

fn parse_sidecar_args(
    args: impl IntoIterator<Item = String>,
) -> Result<RuntimeSupervisorSidecarArgs, CommandError> {
    let mut project_id = None;
    let mut repo_root = None;
    let mut runtime_kind = None;
    let mut run_id = None;
    let mut session_id = None;
    let mut flow_id = None;
    let mut program = None;
    let mut command_args = Vec::new();

    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--project-id" => project_id = args.next(),
            "--repo-root" => repo_root = args.next().map(PathBuf::from),
            "--runtime-kind" => runtime_kind = args.next(),
            "--run-id" => run_id = args.next(),
            "--session-id" => session_id = args.next(),
            "--flow-id" => flow_id = args.next(),
            "--program" => program = args.next(),
            "--command-arg" => {
                let Some(value) = args.next() else {
                    return Err(CommandError::user_fixable(
                        "runtime_supervisor_request_invalid",
                        "Cadence received a detached supervisor command arg flag without a value.",
                    ));
                };
                command_args.push(value);
            }
            other => {
                return Err(CommandError::user_fixable(
                    "runtime_supervisor_request_invalid",
                    format!("Cadence received unsupported detached supervisor argument `{other}`."),
                ))
            }
        }
    }

    let args = RuntimeSupervisorSidecarArgs {
        project_id: project_id.ok_or_else(|| CommandError::invalid_request("projectId"))?,
        repo_root: repo_root.ok_or_else(|| CommandError::invalid_request("repoRoot"))?,
        runtime_kind: runtime_kind.ok_or_else(|| CommandError::invalid_request("runtimeKind"))?,
        run_id: run_id.ok_or_else(|| CommandError::invalid_request("runId"))?,
        session_id: session_id.ok_or_else(|| CommandError::invalid_request("sessionId"))?,
        flow_id,
        program: program.ok_or_else(|| CommandError::invalid_request("program"))?,
        args: command_args,
    };

    validate_non_empty(&args.project_id, "projectId")?;
    validate_non_empty(&args.runtime_kind, "runtimeKind")?;
    validate_non_empty(&args.run_id, "runId")?;
    validate_non_empty(&args.session_id, "sessionId")?;
    if let Some(flow_id) = args.flow_id.as_deref() {
        validate_non_empty(flow_id, "flowId")?;
    }
    validate_non_empty(&args.program, "program")?;

    Ok(args)
}

fn run_supervisor_sidecar(args: RuntimeSupervisorSidecarArgs) -> Result<(), CommandError> {
    let listener = TcpListener::bind(("127.0.0.1", 0)).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_bind_failed",
            "Cadence could not bind the detached PTY supervisor control listener.",
        )
    })?;
    listener.set_nonblocking(true).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_bind_failed",
            "Cadence could not configure the detached PTY supervisor control listener.",
        )
    })?;
    let endpoint = listener
        .local_addr()
        .map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_bind_failed",
                "Cadence could not read the detached PTY supervisor control listener address.",
            )
        })?
        .to_string();

    let pty_system = native_pty_system();
    let pair = pty_system.openpty(PtySize::default()).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_pty_failed",
            "Cadence could not allocate a PTY for the detached supervisor.",
        )
    })?;
    let mut builder = CommandBuilder::new(&args.program);
    builder.args(&args.args);
    builder.cwd(&args.repo_root);

    let mut child = match pair.slave.spawn_command(builder) {
        Ok(child) => child,
        Err(_) => {
            emit_startup_message(&SupervisorStartupMessage::Error {
                protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                code: "runtime_supervisor_pty_failed".into(),
                message:
                    "Cadence could not spawn the requested command inside the detached PTY supervisor."
                        .into(),
                retryable: true,
            })?;
            return Ok(());
        }
    };

    let writer = match pair.master.take_writer() {
        Ok(writer) => writer,
        Err(_) => {
            let _ = child.kill();
            emit_startup_message(&SupervisorStartupMessage::Error {
                protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                code: "runtime_supervisor_writer_unavailable".into(),
                message: "Cadence could not take exclusive ownership of the detached PTY writer."
                    .into(),
                retryable: true,
            })?;
            return Ok(());
        }
    };
    let writer: SharedPtyWriter = Arc::new(Mutex::new(writer));

    let child_pid = child.process_id();
    let mut killer = child.clone_killer();
    let started_at = now_timestamp();

    let initial_snapshot = project_store::upsert_runtime_run(
        &args.repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: args.project_id.clone(),
                run_id: args.run_id.clone(),
                runtime_kind: args.runtime_kind.clone(),
                supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                status: RuntimeRunStatus::Running,
                transport: RuntimeRunTransportRecord {
                    kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                    endpoint: endpoint.clone(),
                    liveness: RuntimeRunTransportLiveness::Reachable,
                },
                started_at: started_at.clone(),
                last_heartbeat_at: Some(started_at.clone()),
                stopped_at: None,
                last_error: None,
                updated_at: started_at.clone(),
            },
            checkpoint: None,
        },
    )
    .map_err(|_| {
        let _ = killer.kill();
        CommandError::retryable(
            "runtime_supervisor_persist_failed",
            "Cadence could not persist detached supervisor startup metadata.",
        )
    })?;

    let shared = Arc::new(Mutex::new(SidecarSharedState {
        project_id: args.project_id.clone(),
        run_id: args.run_id.clone(),
        runtime_kind: args.runtime_kind.clone(),
        session_id: args.session_id.clone(),
        flow_id: args.flow_id.clone(),
        endpoint: endpoint.clone(),
        started_at: initial_snapshot.run.started_at.clone(),
        child_pid,
        status: SupervisorProcessStatus::Running,
        stop_requested: false,
        last_heartbeat_at: initial_snapshot.run.last_heartbeat_at.clone(),
        last_checkpoint_sequence: initial_snapshot.last_checkpoint_sequence,
        last_checkpoint_at: initial_snapshot.last_checkpoint_at.clone(),
        last_error: None,
        stopped_at: None,
        next_boundary_serial: 0,
        active_boundary: None,
    }));
    let event_hub = Arc::new(Mutex::new(SupervisorEventHub::default()));
    let persistence_lock = Arc::new(Mutex::new(()));
    let shutdown = Arc::new(AtomicBool::new(false));

    let control_thread = spawn_control_listener(
        listener,
        shared.clone(),
        event_hub.clone(),
        writer.clone(),
        shutdown.clone(),
        killer,
    );

    emit_startup_message(&SupervisorStartupMessage::Ready {
        protocol_version: SUPERVISOR_PROTOCOL_VERSION,
        project_id: args.project_id.clone(),
        run_id: args.run_id.clone(),
        supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
        transport_kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
        endpoint: endpoint.clone(),
        started_at: initial_snapshot.run.started_at.clone(),
        supervisor_pid: std::process::id(),
        child_pid,
        status: SupervisorProcessStatus::Running,
    })?;

    let reader_thread = spawn_pty_reader(
        pair.master.try_clone_reader().map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_pty_failed",
                "Cadence could not clone the detached PTY supervisor reader.",
            )
        })?,
        args.repo_root.clone(),
        shared.clone(),
        event_hub.clone(),
        persistence_lock.clone(),
        shutdown.clone(),
    );
    let heartbeat_thread = spawn_heartbeat_loop(
        args.repo_root.clone(),
        shared.clone(),
        persistence_lock.clone(),
        shutdown.clone(),
    );

    let exit_status = child.wait().map_err(|_| {
        shutdown.store(true, Ordering::SeqCst);
        CommandError::retryable(
            "runtime_supervisor_wait_failed",
            "Cadence lost the detached PTY child before it returned an exit status.",
        )
    })?;

    persist_sidecar_exit(&args.repo_root, &shared, &persistence_lock, exit_status)?;
    thread::sleep(TERMINAL_ATTACH_GRACE_PERIOD);
    shutdown.store(true, Ordering::SeqCst);

    let _ = control_thread.join();
    let _ = reader_thread.join();
    let _ = heartbeat_thread.join();

    Ok(())
}

fn emit_startup_message(message: &SupervisorStartupMessage) -> Result<(), CommandError> {
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    write_json_line(&mut stdout, message).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_handshake_write_failed",
            "Cadence could not emit the detached PTY supervisor startup handshake.",
        )
    })
}

fn spawn_control_listener(
    listener: TcpListener,
    shared: Arc<Mutex<SidecarSharedState>>,
    event_hub: Arc<Mutex<SupervisorEventHub>>,
    writer: SharedPtyWriter,
    shutdown: Arc<AtomicBool>,
    killer: Box<dyn ChildKiller + Send + Sync>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let killer = Arc::new(Mutex::new(killer));
        while !shutdown.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let shared = shared.clone();
                    let event_hub = event_hub.clone();
                    let writer = writer.clone();
                    let shutdown = shutdown.clone();
                    let killer = killer.clone();
                    thread::spawn(move || {
                        let _ = handle_control_connection(
                            stream, &shared, &event_hub, &writer, &shutdown, &killer,
                        );
                    });
                }
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock
                            | std::io::ErrorKind::Interrupted
                            | std::io::ErrorKind::ConnectionAborted
                    ) =>
                {
                    thread::sleep(CONTROL_ACCEPT_POLL_INTERVAL);
                }
                Err(_) => {
                    if shutdown.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(CONTROL_ACCEPT_POLL_INTERVAL);
                }
            }
        }
    })
}

fn handle_control_connection(
    mut stream: TcpStream,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    writer: &SharedPtyWriter,
    shutdown: &Arc<AtomicBool>,
    killer: &Arc<Mutex<Box<dyn ChildKiller + Send + Sync>>>,
) -> Result<(), CommandError> {
    stream.set_nonblocking(false).map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not configure blocking detached supervisor control IO.",
        )
    })?;
    stream
        .set_read_timeout(Some(DEFAULT_CONTROL_TIMEOUT))
        .map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_control_io_failed",
                "Cadence could not configure the detached supervisor control read timeout.",
            )
        })?;
    stream
        .set_write_timeout(Some(DEFAULT_CONTROL_TIMEOUT))
        .map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_control_io_failed",
                "Cadence could not configure the detached supervisor control write timeout.",
            )
        })?;

    let request = read_json_line_from_reader::<_, SupervisorControlRequest>(
        stream.try_clone().map_err(|_| {
            CommandError::retryable(
                "runtime_supervisor_control_io_failed",
                "Cadence could not clone the detached supervisor control stream.",
            )
        })?,
    );

    match request {
        Ok(SupervisorControlRequest::Probe {
            protocol_version,
            project_id,
            run_id,
        }) => {
            let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                write_protocol_error(
                    &mut stream,
                    "runtime_supervisor_protocol_invalid",
                    "Detached supervisor protocol version mismatch.",
                    false,
                )?;
                return Ok(());
            }

            if project_id != snapshot.project_id || run_id != snapshot.run_id {
                write_protocol_error(
                    &mut stream,
                    "runtime_supervisor_identity_mismatch",
                    "Detached supervisor identity mismatch.",
                    false,
                )?;
                return Ok(());
            }

            write_json_line(
                &mut stream,
                &SupervisorControlResponse::ProbeResult {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id: snapshot.project_id,
                    run_id: snapshot.run_id,
                    status: snapshot.status,
                    last_heartbeat_at: snapshot.last_heartbeat_at,
                    last_checkpoint_sequence: snapshot.last_checkpoint_sequence,
                    last_checkpoint_at: snapshot.last_checkpoint_at,
                    last_error: snapshot.last_error,
                    child_pid: snapshot.child_pid,
                },
            )
            .map_err(|_| {
                CommandError::retryable(
                    "runtime_supervisor_control_io_failed",
                    "Cadence could not write the detached supervisor probe response.",
                )
            })
        }
        Ok(SupervisorControlRequest::Stop {
            protocol_version,
            project_id,
            run_id,
        }) => {
            let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
            if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
                write_protocol_error(
                    &mut stream,
                    "runtime_supervisor_protocol_invalid",
                    "Detached supervisor protocol version mismatch.",
                    false,
                )?;
                return Ok(());
            }

            if project_id != snapshot.project_id || run_id != snapshot.run_id {
                write_protocol_error(
                    &mut stream,
                    "runtime_supervisor_identity_mismatch",
                    "Detached supervisor identity mismatch.",
                    false,
                )?;
                return Ok(());
            }

            {
                let mut snapshot = shared.lock().expect("sidecar state lock poisoned");
                snapshot.stop_requested = true;
            }
            killer
                .lock()
                .expect("detached supervisor killer lock poisoned")
                .kill()
                .map_err(|_| {
                    CommandError::retryable(
                        "runtime_supervisor_stop_failed",
                        "Cadence could not signal the detached PTY child to stop.",
                    )
                })?;
            write_json_line(
                &mut stream,
                &SupervisorControlResponse::StopAccepted {
                    protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                    project_id: snapshot.project_id,
                    run_id: snapshot.run_id,
                    child_pid: snapshot.child_pid,
                },
            )
            .map_err(|_| {
                CommandError::retryable(
                    "runtime_supervisor_control_io_failed",
                    "Cadence could not write the detached supervisor stop acknowledgement.",
                )
            })
        }
        Ok(SupervisorControlRequest::Attach {
            protocol_version,
            project_id,
            run_id,
            after_sequence,
        }) => handle_attach_request(
            &mut stream,
            shared,
            event_hub,
            shutdown,
            protocol_version,
            project_id,
            run_id,
            after_sequence,
        ),
        Ok(SupervisorControlRequest::SubmitInput {
            protocol_version,
            project_id,
            run_id,
            session_id,
            flow_id,
            action_id,
            boundary_id,
            input,
        }) => handle_submit_input_request(
            &mut stream,
            shared,
            event_hub,
            writer,
            protocol_version,
            project_id,
            run_id,
            session_id,
            flow_id,
            action_id,
            boundary_id,
            input,
        ),
        Err(error) => write_protocol_error(
            &mut stream,
            "runtime_supervisor_request_invalid",
            &format!("Cadence rejected a malformed detached supervisor control request: {error}."),
            false,
        ),
    }
}

fn handle_attach_request(
    stream: &mut TcpStream,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    shutdown: &Arc<AtomicBool>,
    protocol_version: u8,
    project_id: String,
    run_id: String,
    after_sequence: Option<u64>,
) -> Result<(), CommandError> {
    if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
        write_protocol_error(
            stream,
            "runtime_supervisor_protocol_invalid",
            "Detached supervisor protocol version mismatch.",
            false,
        )?;
        return Ok(());
    }

    let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
    if project_id != snapshot.project_id || run_id != snapshot.run_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_identity_mismatch",
            "Detached supervisor identity mismatch.",
            false,
        )?;
        return Ok(());
    }

    if matches!(after_sequence, Some(0)) {
        write_protocol_error(
            stream,
            "runtime_supervisor_attach_cursor_invalid",
            "Detached supervisor attach cursors must be greater than zero when provided.",
            false,
        )?;
        return Ok(());
    }

    let terminal_snapshot = matches!(
        snapshot.status,
        SupervisorProcessStatus::Stopped | SupervisorProcessStatus::Failed
    );

    if terminal_snapshot {
        write_protocol_error(
            stream,
            "runtime_supervisor_attach_unavailable",
            "Cadence cannot attach to a detached supervisor after the run reached terminal state.",
            false,
        )?;
        return Ok(());
    }

    let (registration, receiver) = register_attach_replay(event_hub, &snapshot, after_sequence);
    write_json_line(stream, &registration.attach_response).map_err(|_| {
        remove_event_subscriber(event_hub, registration.subscriber_id);
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not write the detached supervisor attach acknowledgement.",
        )
    })?;

    for event in &registration.replay_events {
        let response = live_event_response(event, true);
        if write_json_line(stream, &response).is_err() {
            remove_event_subscriber(event_hub, registration.subscriber_id);
            return Ok(());
        }
    }

    while !shutdown.load(Ordering::SeqCst) {
        match receiver.recv_timeout(CONTROL_ACCEPT_POLL_INTERVAL) {
            Ok(event) => {
                let response = live_event_response(&event, false);
                if write_json_line(stream, &response).is_err() {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    remove_event_subscriber(event_hub, registration.subscriber_id);
    Ok(())
}

fn register_attach_replay(
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    snapshot: &SidecarSharedState,
    after_sequence: Option<u64>,
) -> (
    ReplayRegistration,
    std::sync::mpsc::Receiver<BufferedSupervisorEvent>,
) {
    let (sender, receiver) = sync_channel(LIVE_EVENT_SUBSCRIBER_BUFFER);
    let mut hub = event_hub.lock().expect("event hub lock poisoned");
    hub.next_subscriber_id = hub.next_subscriber_id.saturating_add(1);
    let subscriber_id = hub.next_subscriber_id;
    hub.subscribers.insert(subscriber_id, sender);

    let oldest_available_sequence = hub.ring.front().map(|event| event.sequence);
    let latest_sequence = hub.ring.back().map(|event| event.sequence);
    let replay_events = hub
        .ring
        .iter()
        .filter(|event| after_sequence.map_or(true, |cursor| event.sequence > cursor))
        .cloned()
        .collect::<Vec<_>>();
    let replay_truncated = after_sequence.map_or(
        oldest_available_sequence.is_some_and(|oldest| oldest > 1),
        |cursor| oldest_available_sequence.is_some_and(|oldest| cursor.saturating_add(1) < oldest),
    );

    (
        ReplayRegistration {
            subscriber_id,
            attach_response: SupervisorControlResponse::Attached {
                protocol_version: SUPERVISOR_PROTOCOL_VERSION,
                project_id: snapshot.project_id.clone(),
                run_id: snapshot.run_id.clone(),
                after_sequence,
                replayed_count: replay_events.len() as u32,
                replay_truncated,
                oldest_available_sequence,
                latest_sequence,
            },
            replay_events,
        },
        receiver,
    )
}

fn live_event_response(event: &BufferedSupervisorEvent, replay: bool) -> SupervisorControlResponse {
    SupervisorControlResponse::Event {
        protocol_version: SUPERVISOR_PROTOCOL_VERSION,
        project_id: event.project_id.clone(),
        run_id: event.run_id.clone(),
        sequence: event.sequence,
        created_at: event.created_at.clone(),
        replay,
        item: event.item.clone(),
    }
}

fn remove_event_subscriber(event_hub: &Arc<Mutex<SupervisorEventHub>>, subscriber_id: u64) {
    event_hub
        .lock()
        .expect("event hub lock poisoned")
        .subscribers
        .remove(&subscriber_id);
}

fn handle_submit_input_request(
    stream: &mut TcpStream,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    writer: &SharedPtyWriter,
    protocol_version: u8,
    project_id: String,
    run_id: String,
    session_id: String,
    flow_id: Option<String>,
    action_id: String,
    boundary_id: String,
    input: String,
) -> Result<(), CommandError> {
    if protocol_version != SUPERVISOR_PROTOCOL_VERSION {
        write_protocol_error(
            stream,
            "runtime_supervisor_protocol_invalid",
            "Detached supervisor protocol version mismatch.",
            false,
        )?;
        return Ok(());
    }

    let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
    if project_id != snapshot.project_id || run_id != snapshot.run_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_identity_mismatch",
            "Detached supervisor identity mismatch.",
            false,
        )?;
        return Ok(());
    }

    if session_id != snapshot.session_id || flow_id != snapshot.flow_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_session_mismatch",
            "Detached supervisor session identity mismatch.",
            false,
        )?;
        return Ok(());
    }

    let Some(active_boundary) = snapshot.active_boundary.clone() else {
        write_protocol_error(
            stream,
            "runtime_supervisor_action_unavailable",
            "Cadence cannot deliver terminal input because no interactive boundary is currently pending.",
            false,
        )?;
        return Ok(());
    };

    if action_id != active_boundary.action_id || boundary_id != active_boundary.boundary_id {
        write_protocol_error(
            stream,
            "runtime_supervisor_action_mismatch",
            "Cadence rejected terminal input for a stale or mismatched interactive boundary.",
            false,
        )?;
        return Ok(());
    }

    let input = match normalize_control_input(&input) {
        Ok(input) => input,
        Err(error) => {
            write_protocol_error(stream, &error.code, &error.message, error.retryable)?;
            return Ok(());
        }
    };

    let mut writer = writer
        .lock()
        .expect("runtime supervisor writer lock poisoned");
    if writer.write_all(input.as_bytes()).is_err()
        || writer
            .write_all(if input.ends_with('\n') { b"" } else { b"\n" })
            .is_err()
        || writer.flush().is_err()
    {
        write_protocol_error(
            stream,
            "runtime_supervisor_submit_input_failed",
            "Cadence could not write approved terminal input into the detached PTY.",
            true,
        )?;
        return Ok(());
    }
    drop(writer);

    {
        let mut state = shared.lock().expect("sidecar state lock poisoned");
        if state
            .active_boundary
            .as_ref()
            .is_some_and(|boundary| boundary.action_id == action_id)
        {
            state.active_boundary = None;
        }
    }

    let delivered_at = now_timestamp();
    append_live_event(
        shared,
        event_hub,
        &SupervisorLiveEventPayload::Activity {
            code: "runtime_supervisor_input_delivered".into(),
            title: "Terminal input delivered".into(),
            detail: Some(
                "Cadence wrote approved operator input into the active detached PTY.".into(),
            ),
        },
    );

    write_json_line(
        stream,
        &SupervisorControlResponse::SubmitInputAccepted {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            project_id: snapshot.project_id,
            run_id: snapshot.run_id,
            action_id,
            boundary_id,
            delivered_at,
        },
    )
    .map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not write the detached supervisor submit-input acknowledgement.",
        )
    })
}

fn normalize_control_input(input: &str) -> Result<String, CommandError> {
    let normalized = input.trim_end_matches(['\r', '\n']);
    if normalized.trim().is_empty() {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_submit_input_invalid",
            "Cadence requires non-empty terminal input before it can resume the detached PTY.",
        ));
    }

    if normalized.chars().count() > MAX_CONTROL_INPUT_CHARS {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_submit_input_invalid",
            "Cadence refused oversized terminal input for the detached PTY.",
        ));
    }

    Ok(normalized.to_string())
}

fn write_protocol_error(
    stream: &mut TcpStream,
    code: &str,
    message: &str,
    retryable: bool,
) -> Result<(), CommandError> {
    write_json_line(
        stream,
        &SupervisorControlResponse::Error {
            protocol_version: SUPERVISOR_PROTOCOL_VERSION,
            code: code.into(),
            message: message.into(),
            retryable,
        },
    )
    .map_err(|_| {
        CommandError::retryable(
            "runtime_supervisor_control_io_failed",
            "Cadence could not write the detached supervisor control error response.",
        )
    })
}

impl PtyEventNormalizer {
    fn push_chunk(&mut self, chunk: &[u8]) -> Vec<NormalizedPtyEvent> {
        self.pending.extend_from_slice(chunk);
        self.drain_complete_lines(false)
    }

    fn finish(&mut self) -> Vec<NormalizedPtyEvent> {
        self.drain_complete_lines(true)
    }

    fn take_interactive_boundary_candidate(&mut self) -> Option<InteractiveBoundaryCandidate> {
        if self.pending.is_empty() {
            return None;
        }

        let pending = String::from_utf8(self.pending.clone()).ok()?;
        let fragment = pending.trim_end_matches(['\r', '\n']);
        let sanitized = match sanitize_text_fragment(fragment) {
            Ok(Some(text)) => text,
            Ok(None) | Err(()) => return None,
        };

        if !looks_like_interactive_boundary(&sanitized) {
            return None;
        }

        self.pending.clear();
        Some(default_interactive_boundary_candidate())
    }

    fn drain_complete_lines(&mut self, flush_partial: bool) -> Vec<NormalizedPtyEvent> {
        let mut events = Vec::new();

        loop {
            let Some(newline_index) = self.pending.iter().position(|byte| *byte == b'\n') else {
                if self.pending.len() > MAX_LIVE_EVENT_FRAGMENT_BYTES {
                    self.pending.clear();
                    events.push(diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized detached PTY output fragment before replay.",
                    ));
                } else if flush_partial && !self.pending.is_empty() {
                    let remainder = std::mem::take(&mut self.pending);
                    events.extend(normalize_pty_line_bytes(&remainder));
                }
                break;
            };

            let mut line = self.pending.drain(..=newline_index).collect::<Vec<_>>();
            if matches!(line.last(), Some(b'\n')) {
                line.pop();
            }
            if matches!(line.last(), Some(b'\r')) {
                line.pop();
            }
            events.extend(normalize_pty_line_bytes(&line));
        }

        events
    }
}

fn normalize_pty_line_bytes(raw_line: &[u8]) -> Vec<NormalizedPtyEvent> {
    if raw_line.is_empty() {
        return Vec::new();
    }

    let line = match String::from_utf8(raw_line.to_vec()) {
        Ok(line) => line,
        Err(_) => {
            return vec![diagnostic_live_event(
                "runtime_supervisor_live_event_decode_failed",
                "Live output decode failed",
                "Cadence dropped a detached PTY output fragment that was not valid UTF-8.",
            )];
        }
    };

    normalize_pty_line(&line).into_iter().collect()
}

fn normalize_pty_line(raw_line: &str) -> Option<NormalizedPtyEvent> {
    let trimmed = raw_line.trim_end();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(payload) = trimmed.strip_prefix(STRUCTURED_EVENT_PREFIX) {
        return Some(normalize_structured_event(payload));
    }

    let text = match sanitize_text_fragment(trimmed) {
        Ok(Some(text)) => text,
        Ok(None) => return None,
        Err(()) => {
            return Some(diagnostic_live_event(
                "runtime_supervisor_live_event_oversized",
                "Live output fragment dropped",
                "Cadence dropped an oversized detached PTY output fragment before replay.",
            ))
        }
    };

    if contains_prohibited_live_content(&text).is_some() {
        return Some(redacted_live_event());
    }

    Some(NormalizedPtyEvent {
        checkpoint_summary: summarize_pty_output(&text),
        item: SupervisorLiveEventPayload::Transcript { text },
    })
}

fn default_interactive_boundary_candidate() -> InteractiveBoundaryCandidate {
    InteractiveBoundaryCandidate {
        action_type: INTERACTIVE_BOUNDARY_ACTION_TYPE.into(),
        title: INTERACTIVE_BOUNDARY_TITLE.into(),
        detail: INTERACTIVE_BOUNDARY_DETAIL.into(),
        checkpoint_summary: INTERACTIVE_BOUNDARY_CHECKPOINT_SUMMARY.into(),
    }
}

fn looks_like_interactive_boundary(fragment: &str) -> bool {
    let trimmed = fragment.trim();
    if trimmed.is_empty() || trimmed.chars().count() > 160 {
        return false;
    }

    let normalized = trimmed.to_ascii_lowercase();
    if matches!(normalized.as_str(), "$" | "#" | "%" | ">") {
        return false;
    }

    let has_prompt_suffix = matches!(trimmed.chars().last(), Some(':' | '?' | '>' | ']' | ')'));
    if !has_prompt_suffix {
        return false;
    }

    let has_prompt_keyword = [
        "enter",
        "input",
        "provide",
        "type",
        "passphrase",
        "password",
        "token",
        "code",
        "continue",
        "confirm",
        "approve",
        "answer",
        "select",
        "choose",
        "name",
        "email",
        "y/n",
        "yes/no",
    ]
    .into_iter()
    .any(|keyword| normalized.contains(keyword));

    let looks_like_prompt_sentence = trimmed.contains(' ') || has_prompt_keyword;
    let looks_like_log_prefix = normalized.starts_with("error:")
        || normalized.starts_with("warning:")
        || normalized.starts_with("info:");

    looks_like_prompt_sentence && !looks_like_log_prefix
}

fn normalize_structured_event(payload: &str) -> NormalizedPtyEvent {
    if payload.trim().is_empty() {
        return diagnostic_live_event(
            "runtime_supervisor_live_event_blank",
            "Live output fragment dropped",
            "Cadence dropped a blank structured live-event payload before replay.",
        );
    }

    if payload.len() > MAX_LIVE_EVENT_FRAGMENT_BYTES {
        return diagnostic_live_event(
            "runtime_supervisor_live_event_oversized",
            "Live output fragment dropped",
            "Cadence dropped an oversized structured live-event payload before replay.",
        );
    }

    let value = match serde_json::from_str::<serde_json::Value>(payload) {
        Ok(value) => value,
        Err(_) => {
            return diagnostic_live_event(
                "runtime_supervisor_live_event_invalid",
                "Live output fragment dropped",
                "Cadence dropped a malformed structured live-event payload before replay.",
            );
        }
    };

    let Some(kind) = value.get("kind").and_then(serde_json::Value::as_str) else {
        return diagnostic_live_event(
            "runtime_supervisor_live_event_invalid",
            "Live output fragment dropped",
            "Cadence dropped a structured live-event payload without a kind.",
        );
    };

    match kind {
        "transcript" => {
            let Some(text) = value.get("text").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured transcript payload without text.",
                );
            };
            let text =
                match sanitize_text_fragment(text) {
                    Ok(Some(text)) => text,
                    Ok(None) => {
                        return diagnostic_live_event(
                            "runtime_supervisor_live_event_blank",
                            "Live output fragment dropped",
                            "Cadence dropped a blank structured transcript payload before replay.",
                        )
                    }
                    Err(()) => return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured transcript payload before replay.",
                    ),
                };
            if contains_prohibited_live_content(&text).is_some() {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: summarize_pty_output(&text),
                    item: SupervisorLiveEventPayload::Transcript { text },
                }
            }
        }
        "tool" => {
            let Some(tool_call_id) = value
                .get("tool_call_id")
                .and_then(serde_json::Value::as_str)
            else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload without a tool_call_id.",
                );
            };
            let Some(tool_name) = value.get("tool_name").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload without a tool_name.",
                );
            };
            let Some(tool_state) = value.get("tool_state").and_then(serde_json::Value::as_str)
            else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload without a tool_state.",
                );
            };
            let Some(tool_call_id) = sanitize_identifier_fragment(tool_call_id) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload with a blank tool_call_id.",
                );
            };
            let Some(tool_name) = sanitize_identifier_fragment(tool_name) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured tool payload with a blank tool_name.",
                );
            };
            let tool_state = match tool_state {
                "pending" => SupervisorToolCallState::Pending,
                "running" => SupervisorToolCallState::Running,
                "succeeded" => SupervisorToolCallState::Succeeded,
                "failed" => SupervisorToolCallState::Failed,
                _ => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_unsupported",
                        "Live output fragment dropped",
                        "Cadence dropped a structured tool payload with an unsupported tool_state.",
                    );
                }
            };
            let detail = value
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .map(sanitize_text_fragment)
                .transpose();
            let detail = match detail {
                Ok(detail) => detail.flatten(),
                Err(_) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured tool detail before replay.",
                    );
                }
            };
            let tool_summary = value
                .get("tool_summary")
                .map(sanitize_tool_result_summary_value)
                .transpose();
            let tool_summary = match tool_summary {
                Ok(tool_summary) => tool_summary,
                Err(ToolSummaryDecodeError::Oversized) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured tool summary before replay.",
                    );
                }
                Err(ToolSummaryDecodeError::Unsupported) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_unsupported",
                        "Live output fragment dropped",
                        "Cadence dropped a structured tool payload with an unsupported tool_summary kind.",
                    );
                }
                Err(ToolSummaryDecodeError::Invalid) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_invalid",
                        "Live output fragment dropped",
                        "Cadence dropped a structured tool payload with invalid tool_summary metadata.",
                    );
                }
            };
            if [
                Some(tool_call_id.as_str()),
                Some(tool_name.as_str()),
                detail.as_deref(),
            ]
            .into_iter()
            .flatten()
            .chain(
                tool_summary
                    .as_ref()
                    .into_iter()
                    .flat_map(tool_result_summary_text_fragments),
            )
            .any(|value| contains_prohibited_live_content(value).is_some())
            {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: Some(tool_checkpoint_summary(&tool_name, &tool_state)),
                    item: SupervisorLiveEventPayload::Tool {
                        tool_call_id,
                        tool_name,
                        tool_state,
                        detail,
                        tool_summary,
                    },
                }
            }
        }
        "activity" => {
            let Some(code) = value.get("code").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured activity payload without a code.",
                );
            };
            let Some(title) = value.get("title").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured activity payload without a title.",
                );
            };
            let Some(code) = sanitize_identifier_fragment(code) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured activity payload with a blank code.",
                );
            };
            let title = match sanitize_text_fragment(title) {
                Ok(Some(title)) => title,
                Ok(None) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_blank",
                        "Live output fragment dropped",
                        "Cadence dropped a structured activity payload with a blank title.",
                    )
                }
                Err(()) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured activity title before replay.",
                    )
                }
            };
            let detail = value
                .get("detail")
                .and_then(serde_json::Value::as_str)
                .map(sanitize_text_fragment)
                .transpose();
            let detail = match detail {
                Ok(detail) => detail.flatten(),
                Err(_) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_oversized",
                        "Live output fragment dropped",
                        "Cadence dropped an oversized structured activity detail before replay.",
                    );
                }
            };
            if [Some(code.as_str()), Some(title.as_str()), detail.as_deref()]
                .into_iter()
                .flatten()
                .any(|value| contains_prohibited_live_content(value).is_some())
            {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: Some(activity_checkpoint_summary(&code, &title)),
                    item: SupervisorLiveEventPayload::Activity {
                        code,
                        title,
                        detail,
                    },
                }
            }
        }
        "action_required" => {
            let Some(action_id) = value.get("action_id").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without an action_id.",
                );
            };
            let Some(boundary_id) = value.get("boundary_id").and_then(serde_json::Value::as_str)
            else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without a boundary_id.",
                );
            };
            let Some(action_type) = value.get("action_type").and_then(serde_json::Value::as_str)
            else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without an action_type.",
                );
            };
            let Some(title) = value.get("title").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without a title.",
                );
            };
            let Some(detail) = value.get("detail").and_then(serde_json::Value::as_str) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_invalid",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload without detail.",
                );
            };

            let Some(action_id) = sanitize_identifier_fragment(action_id) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload with a blank action_id.",
                );
            };
            let Some(boundary_id) = sanitize_identifier_fragment(boundary_id) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload with a blank boundary_id.",
                );
            };
            let Some(action_type) = sanitize_identifier_fragment(action_type) else {
                return diagnostic_live_event(
                    "runtime_supervisor_live_event_blank",
                    "Live output fragment dropped",
                    "Cadence dropped a structured action-required payload with a blank action_type.",
                );
            };
            let title = match sanitize_text_fragment(title) {
                Ok(Some(title)) => title,
                Ok(None) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_blank",
                        "Live output fragment dropped",
                        "Cadence dropped a structured action-required payload with a blank title.",
                    )
                }
                Err(()) => return diagnostic_live_event(
                    "runtime_supervisor_live_event_oversized",
                    "Live output fragment dropped",
                    "Cadence dropped an oversized structured action-required title before replay.",
                ),
            };
            let detail = match sanitize_text_fragment(detail) {
                Ok(Some(detail)) => detail,
                Ok(None) => {
                    return diagnostic_live_event(
                        "runtime_supervisor_live_event_blank",
                        "Live output fragment dropped",
                        "Cadence dropped a structured action-required payload with blank detail.",
                    )
                }
                Err(()) => return diagnostic_live_event(
                    "runtime_supervisor_live_event_oversized",
                    "Live output fragment dropped",
                    "Cadence dropped an oversized structured action-required detail before replay.",
                ),
            };
            if [
                Some(action_id.as_str()),
                Some(boundary_id.as_str()),
                Some(action_type.as_str()),
                Some(title.as_str()),
                Some(detail.as_str()),
            ]
            .into_iter()
            .flatten()
            .any(|value| contains_prohibited_live_content(value).is_some())
            {
                redacted_live_event()
            } else {
                NormalizedPtyEvent {
                    checkpoint_summary: Some(INTERACTIVE_BOUNDARY_CHECKPOINT_SUMMARY.into()),
                    item: SupervisorLiveEventPayload::ActionRequired {
                        action_id,
                        boundary_id,
                        action_type,
                        title,
                        detail,
                    },
                }
            }
        }
        _ => diagnostic_live_event(
            "runtime_supervisor_live_event_unsupported",
            "Live output fragment dropped",
            "Cadence dropped a structured live-event payload with an unsupported kind.",
        ),
    }
}

fn sanitize_identifier_fragment(raw: &str) -> Option<String> {
    let value = raw.trim();
    if value.is_empty() {
        return None;
    }
    if value.chars().count() > MAX_LIVE_EVENT_TEXT_CHARS {
        return None;
    }
    Some(value.to_string())
}

fn sanitize_text_fragment(raw: &str) -> Result<Option<String>, ()> {
    let sanitized = raw
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => ' ',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();
    let collapsed = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > MAX_LIVE_EVENT_TEXT_CHARS {
        return Err(());
    }
    Ok(Some(trimmed.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolSummaryDecodeError {
    Invalid,
    Oversized,
    Unsupported,
}

fn sanitize_tool_result_summary_value(
    value: &serde_json::Value,
) -> Result<ToolResultSummary, ToolSummaryDecodeError> {
    let parsed = serde_json::from_value::<ToolResultSummary>(value.clone()).map_err(|error| {
        let details = error.to_string();
        if details.contains("unknown variant") {
            ToolSummaryDecodeError::Unsupported
        } else {
            ToolSummaryDecodeError::Invalid
        }
    })?;
    sanitize_tool_result_summary(parsed)
}

fn sanitize_tool_result_summary(
    summary: ToolResultSummary,
) -> Result<ToolResultSummary, ToolSummaryDecodeError> {
    match summary {
        ToolResultSummary::Command(summary) => Ok(ToolResultSummary::Command(summary)),
        ToolResultSummary::File(summary) => Ok(ToolResultSummary::File(FileToolResultSummary {
            path: sanitize_optional_tool_summary_text(summary.path)?,
            scope: sanitize_optional_tool_summary_text(summary.scope)?,
            line_count: summary.line_count,
            match_count: summary.match_count,
            truncated: summary.truncated,
        })),
        ToolResultSummary::Git(summary) => Ok(ToolResultSummary::Git(GitToolResultSummary {
            scope: summary.scope,
            changed_files: summary.changed_files,
            truncated: summary.truncated,
            base_revision: sanitize_optional_tool_summary_text(summary.base_revision)?,
        })),
        ToolResultSummary::Web(summary) => Ok(ToolResultSummary::Web(WebToolResultSummary {
            target: sanitize_required_tool_summary_text(summary.target)?,
            result_count: summary.result_count,
            final_url: sanitize_optional_tool_summary_text(summary.final_url)?,
            content_kind: summary.content_kind,
            content_type: sanitize_optional_tool_summary_text(summary.content_type)?,
            truncated: summary.truncated,
        })),
    }
}

fn sanitize_optional_tool_summary_text(
    value: Option<String>,
) -> Result<Option<String>, ToolSummaryDecodeError> {
    value
        .map(|value| match sanitize_text_fragment(&value) {
            Ok(sanitized) => Ok(sanitized),
            Err(()) => Err(ToolSummaryDecodeError::Oversized),
        })
        .transpose()
        .map(|value| value.flatten())
}

fn sanitize_required_tool_summary_text(value: String) -> Result<String, ToolSummaryDecodeError> {
    match sanitize_text_fragment(&value) {
        Ok(Some(value)) => Ok(value),
        Ok(None) => Err(ToolSummaryDecodeError::Invalid),
        Err(()) => Err(ToolSummaryDecodeError::Oversized),
    }
}

fn tool_result_summary_text_fragments(summary: &ToolResultSummary) -> Vec<&str> {
    match summary {
        ToolResultSummary::Command(CommandToolResultSummary { .. }) => Vec::new(),
        ToolResultSummary::File(summary) => [summary.path.as_deref(), summary.scope.as_deref()]
            .into_iter()
            .flatten()
            .collect(),
        ToolResultSummary::Git(GitToolResultSummary { base_revision, .. }) => {
            base_revision.iter().map(String::as_str).collect()
        }
        ToolResultSummary::Web(summary) => [
            Some(summary.target.as_str()),
            summary.final_url.as_deref(),
            summary.content_type.as_deref(),
        ]
        .into_iter()
        .flatten()
        .collect(),
    }
}

fn contains_prohibited_live_content(value: &str) -> Option<&'static str> {
    let normalized = value.to_ascii_lowercase();

    if normalized.contains("access_token")
        || normalized.contains("refresh_token")
        || normalized.contains("bearer ")
        || normalized.contains("oauth")
        || normalized.contains("sk-")
    {
        return Some("OAuth or API token material");
    }

    if normalized.contains("redirect_uri")
        || normalized.contains("authorization_url")
        || normalized.contains("/auth/callback")
        || normalized.contains("127.0.0.1:")
        || normalized.contains("localhost:")
    {
        return Some("OAuth redirect URL data");
    }

    if normalized.contains("chatgpt_account_id")
        || (normalized.contains("session_id") && normalized.contains("provider_id"))
    {
        return Some("auth-store contents");
    }

    None
}

fn redacted_live_event() -> NormalizedPtyEvent {
    diagnostic_live_event(
        "runtime_supervisor_live_event_redacted",
        "Live output redacted",
        REDACTED_LIVE_EVENT_DETAIL,
    )
}

fn diagnostic_live_event(code: &str, title: &str, detail: &str) -> NormalizedPtyEvent {
    NormalizedPtyEvent {
        checkpoint_summary: Some(activity_checkpoint_summary(code, title)),
        item: SupervisorLiveEventPayload::Activity {
            code: code.into(),
            title: title.into(),
            detail: Some(detail.into()),
        },
    }
}

fn tool_checkpoint_summary(tool_name: &str, tool_state: &SupervisorToolCallState) -> String {
    let state = match tool_state {
        SupervisorToolCallState::Pending => "pending",
        SupervisorToolCallState::Running => "running",
        SupervisorToolCallState::Succeeded => "succeeded",
        SupervisorToolCallState::Failed => "failed",
    };
    format!("Tool `{tool_name}` {state}.")
}

fn activity_checkpoint_summary(code: &str, title: &str) -> String {
    format!("{ACTIVITY_OUTPUT_PREFIX} {code}: {title}")
}

fn emit_normalized_events(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    events: Vec<NormalizedPtyEvent>,
) {
    for event in events {
        let buffered = append_live_event(shared, event_hub, &event.item);
        if let Some(summary) = event
            .checkpoint_summary
            .filter(|summary| should_persist_live_event_checkpoint(&buffered, summary))
        {
            let _ = persist_sidecar_checkpoint(
                repo_root,
                shared,
                persistence_lock,
                RuntimeRunStatus::Running,
                project_store::RuntimeRunCheckpointKind::State,
                summary,
            );
        }

        let should_persist_autonomous_event = match &event.item {
            SupervisorLiveEventPayload::Tool { .. }
            | SupervisorLiveEventPayload::ActionRequired { .. } => true,
            SupervisorLiveEventPayload::Activity { code, .. } => code.contains("policy_denied"),
            _ => false,
        };
        if should_persist_autonomous_event {
            persist_autonomous_live_event(
                repo_root,
                shared,
                event_hub,
                persistence_lock,
                &event.item,
            );
        }
    }
}

fn persist_autonomous_live_event(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    event: &SupervisorLiveEventPayload,
) {
    let project_id = {
        shared
            .lock()
            .expect("sidecar state lock poisoned")
            .project_id
            .clone()
    };

    let Err(error) =
        super::autonomous_orchestrator::persist_supervisor_event(repo_root, &project_id, event)
    else {
        return;
    };

    let detail = format!(
        "Cadence kept the prior durable autonomous snapshot after rejecting live-event persistence: [{}] {}",
        error.code, error.message,
    );
    append_live_event(
        shared,
        event_hub,
        &SupervisorLiveEventPayload::Activity {
            code: "autonomous_live_event_persist_failed".into(),
            title: "Autonomous live-event persistence deferred".into(),
            detail: Some(detail),
        },
    );
    let _ = persist_sidecar_checkpoint(
        repo_root,
        shared,
        persistence_lock,
        RuntimeRunStatus::Running,
        project_store::RuntimeRunCheckpointKind::State,
        activity_checkpoint_summary(
            "autonomous_live_event_persist_failed",
            "Autonomous live-event persistence deferred",
        ),
    );
}

fn emit_interactive_boundary_if_detected(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: &Arc<Mutex<()>>,
    normalizer: &mut PtyEventNormalizer,
) {
    let Some(candidate) = normalizer.take_interactive_boundary_candidate() else {
        return;
    };

    let (
        project_id,
        run_id,
        runtime_kind,
        session_id,
        flow_id,
        transport_endpoint,
        started_at,
        last_heartbeat_at,
        last_error,
        boundary,
    ) = {
        let mut state = shared.lock().expect("sidecar state lock poisoned");
        if state.active_boundary.is_some() {
            return;
        }
        state.next_boundary_serial = state.next_boundary_serial.saturating_add(1);
        let boundary = ActiveInteractiveBoundary {
            boundary_id: format!("boundary-{}", state.next_boundary_serial),
            action_id: String::new(),
            action_type: candidate.action_type.clone(),
            title: candidate.title.clone(),
            detail: candidate.detail.clone(),
            detected_at: now_timestamp(),
        };
        (
            state.project_id.clone(),
            state.run_id.clone(),
            state.runtime_kind.clone(),
            state.session_id.clone(),
            state.flow_id.clone(),
            state.endpoint.clone(),
            state.started_at.clone(),
            state.last_heartbeat_at.clone(),
            state
                .last_error
                .clone()
                .map(protocol_diagnostic_into_record),
            boundary,
        )
    };

    let persisted = project_store::upsert_runtime_action_required(
        repo_root,
        &RuntimeActionRequiredUpsertRecord {
            project_id,
            run_id,
            runtime_kind,
            session_id,
            flow_id,
            transport_endpoint,
            started_at,
            last_heartbeat_at,
            last_error,
            boundary_id: boundary.boundary_id.clone(),
            action_type: boundary.action_type.clone(),
            title: boundary.title.clone(),
            detail: boundary.detail.clone(),
            checkpoint_summary: candidate.checkpoint_summary.clone(),
            created_at: boundary.detected_at.clone(),
        },
    );

    match persisted {
        Ok(persisted) => {
            let action_id = persisted.approval_request.action_id.clone();
            let boundary_id = boundary.boundary_id.clone();
            let notification_dispatch_outcome = persisted.notification_dispatch_outcome.clone();
            {
                let mut state = shared.lock().expect("sidecar state lock poisoned");
                state.active_boundary = Some(ActiveInteractiveBoundary {
                    action_id: action_id.clone(),
                    ..boundary.clone()
                });
                state.last_checkpoint_sequence = persisted.runtime_run.last_checkpoint_sequence;
                state.last_checkpoint_at = persisted.runtime_run.last_checkpoint_at.clone();
            }

            append_live_event(
                shared,
                event_hub,
                &SupervisorLiveEventPayload::ActionRequired {
                    action_id: action_id.clone(),
                    boundary_id,
                    action_type: candidate.action_type,
                    title: candidate.title,
                    detail: candidate.detail,
                },
            );
            persist_autonomous_live_event(
                repo_root,
                shared,
                event_hub,
                persistence_lock,
                &SupervisorLiveEventPayload::ActionRequired {
                    action_id: action_id.clone(),
                    boundary_id: boundary.boundary_id.clone(),
                    action_type: boundary.action_type.clone(),
                    title: boundary.title.clone(),
                    detail: boundary.detail.clone(),
                },
            );

            match notification_dispatch_outcome.status {
                NotificationDispatchEnqueueStatus::Enqueued => {
                    append_live_event(
                        shared,
                        event_hub,
                        &SupervisorLiveEventPayload::Activity {
                            code: notification_dispatch_outcome
                                .code
                                .unwrap_or_else(|| "notification_dispatch_enqueued".into()),
                            title: "Notification dispatch fan-out enqueued".into(),
                            detail: Some(format!(
                                "Cadence enqueued {} notification dispatch route(s) for pending action `{action_id}`.",
                                notification_dispatch_outcome.dispatch_count
                            )),
                        },
                    );
                }
                NotificationDispatchEnqueueStatus::Skipped => {
                    append_live_event(
                        shared,
                        event_hub,
                        &SupervisorLiveEventPayload::Activity {
                            code: notification_dispatch_outcome
                                .code
                                .unwrap_or_else(|| "notification_dispatch_enqueue_skipped".into()),
                            title: "Notification dispatch fan-out skipped".into(),
                            detail: Some(
                                notification_dispatch_outcome
                                    .message
                                    .unwrap_or_else(|| {
                                        "Cadence skipped notification dispatch fan-out after persisting the pending interactive boundary."
                                            .into()
                                    }),
                            ),
                        },
                    );
                }
            }
        }
        Err(error) => {
            let safe_detail =
                "Cadence could not persist the interactive boundary, so the last truthful runtime snapshot remains active.";
            append_live_event(
                shared,
                event_hub,
                &SupervisorLiveEventPayload::Activity {
                    code: error.code.clone(),
                    title: "Interactive boundary persistence failed".into(),
                    detail: Some(safe_detail.into()),
                },
            );
            let _ = persist_sidecar_runtime_error(
                repo_root,
                shared,
                persistence_lock,
                &error.code,
                safe_detail,
            );
        }
    }
}

fn should_persist_live_event_checkpoint(event: &BufferedSupervisorEvent, _summary: &str) -> bool {
    event.sequence == 1
        || event.sequence % 16 == 0
        || matches!(
            event.item,
            SupervisorLiveEventPayload::Tool { .. }
                | SupervisorLiveEventPayload::Activity { .. }
                | SupervisorLiveEventPayload::ActionRequired { .. }
        )
}

fn append_live_event(
    shared: &Arc<Mutex<SidecarSharedState>>,
    event_hub: &Arc<Mutex<SupervisorEventHub>>,
    item: &SupervisorLiveEventPayload,
) -> BufferedSupervisorEvent {
    let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
    let mut hub = event_hub.lock().expect("event hub lock poisoned");
    hub.next_sequence = hub.next_sequence.saturating_add(1);
    let event = BufferedSupervisorEvent {
        project_id: snapshot.project_id,
        run_id: snapshot.run_id,
        sequence: hub.next_sequence,
        created_at: now_timestamp(),
        item: item.clone(),
    };

    if hub.ring.len() == LIVE_EVENT_RING_LIMIT {
        hub.ring.pop_front();
    }
    hub.ring.push_back(event.clone());

    let mut stale_subscribers = Vec::new();
    for (subscriber_id, sender) in &hub.subscribers {
        match sender.try_send(event.clone()) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) | Err(TrySendError::Disconnected(_)) => {
                stale_subscribers.push(*subscriber_id);
            }
        }
    }

    for subscriber_id in stale_subscribers {
        hub.subscribers.remove(&subscriber_id);
    }

    event
}

fn spawn_pty_reader(
    mut reader: Box<dyn Read + Send>,
    repo_root: PathBuf,
    shared: Arc<Mutex<SidecarSharedState>>,
    event_hub: Arc<Mutex<SupervisorEventHub>>,
    persistence_lock: Arc<Mutex<()>>,
    shutdown: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        let mut normalizer = PtyEventNormalizer::default();

        while !shutdown.load(Ordering::SeqCst) {
            match reader.read(&mut buffer) {
                Ok(0) => {
                    emit_normalized_events(
                        &repo_root,
                        &shared,
                        &event_hub,
                        &persistence_lock,
                        normalizer.finish(),
                    );
                    break;
                }
                Ok(bytes_read) => {
                    emit_normalized_events(
                        &repo_root,
                        &shared,
                        &event_hub,
                        &persistence_lock,
                        normalizer.push_chunk(&buffer[..bytes_read]),
                    );
                    emit_interactive_boundary_if_detected(
                        &repo_root,
                        &shared,
                        &event_hub,
                        &persistence_lock,
                        &mut normalizer,
                    );
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    emit_normalized_events(
                        &repo_root,
                        &shared,
                        &event_hub,
                        &persistence_lock,
                        vec![diagnostic_live_event(
                            "runtime_supervisor_reader_failed",
                            "Runtime stream read failed",
                            "Cadence lost the detached PTY reader before the child exited.",
                        )],
                    );
                    let _ = persist_sidecar_runtime_error(
                        &repo_root,
                        &shared,
                        &persistence_lock,
                        "runtime_supervisor_reader_failed",
                        "Cadence lost the detached PTY reader before the child exited.",
                    );
                    break;
                }
            }
        }
    })
}

fn spawn_heartbeat_loop(
    repo_root: PathBuf,
    shared: Arc<Mutex<SidecarSharedState>>,
    persistence_lock: Arc<Mutex<()>>,
    shutdown: Arc<AtomicBool>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        while !shutdown.load(Ordering::SeqCst) {
            thread::sleep(HEARTBEAT_INTERVAL);
            if shutdown.load(Ordering::SeqCst) {
                break;
            }

            {
                let mut snapshot = shared.lock().expect("sidecar state lock poisoned");
                if matches!(
                    snapshot.status,
                    SupervisorProcessStatus::Stopped | SupervisorProcessStatus::Failed
                ) {
                    break;
                }
                snapshot.last_heartbeat_at = Some(now_timestamp());
            }

            let _ = persist_runtime_row_from_shared(&repo_root, &shared, &persistence_lock);
        }
    })
}

fn persist_sidecar_exit(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    persistence_lock: &Arc<Mutex<()>>,
    exit_status: portable_pty::ExitStatus,
) -> Result<(), CommandError> {
    let stop_requested = shared
        .lock()
        .expect("sidecar state lock poisoned")
        .stop_requested;

    let (status, last_error, summary) = if stop_requested {
        (
            SupervisorProcessStatus::Stopped,
            None,
            "PTY child stopped by supervisor request.".to_string(),
        )
    } else if exit_status.success() {
        (
            SupervisorProcessStatus::Stopped,
            None,
            "PTY child exited cleanly.".to_string(),
        )
    } else {
        (
            SupervisorProcessStatus::Failed,
            Some(SupervisorProtocolDiagnostic {
                code: "runtime_supervisor_exit_nonzero".into(),
                message: format!("PTY child exited with status {exit_status}."),
            }),
            format!("PTY child exited with status {exit_status}."),
        )
    };

    {
        let mut snapshot = shared.lock().expect("sidecar state lock poisoned");
        snapshot.status = status.clone();
        snapshot.last_error = last_error;
        snapshot.stopped_at = Some(now_timestamp());
        snapshot.last_heartbeat_at = Some(now_timestamp());
    }

    persist_runtime_row_from_shared(repo_root, shared, persistence_lock)?;
    persist_sidecar_checkpoint(
        repo_root,
        shared,
        persistence_lock,
        match status {
            SupervisorProcessStatus::Stopped => RuntimeRunStatus::Stopped,
            SupervisorProcessStatus::Failed => RuntimeRunStatus::Failed,
            SupervisorProcessStatus::Starting => RuntimeRunStatus::Starting,
            SupervisorProcessStatus::Running => RuntimeRunStatus::Running,
        },
        project_store::RuntimeRunCheckpointKind::State,
        summary,
    )?;

    Ok(())
}

fn persist_sidecar_runtime_error(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    persistence_lock: &Arc<Mutex<()>>,
    code: &str,
    message: &str,
) -> Result<(), CommandError> {
    {
        let mut snapshot = shared.lock().expect("sidecar state lock poisoned");
        snapshot.last_error = Some(SupervisorProtocolDiagnostic {
            code: code.into(),
            message: message.into(),
        });
    }

    persist_runtime_row_from_shared(repo_root, shared, persistence_lock).map(|_| ())
}

fn persist_sidecar_checkpoint(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    persistence_lock: &Arc<Mutex<()>>,
    status: RuntimeRunStatus,
    checkpoint_kind: project_store::RuntimeRunCheckpointKind,
    summary: String,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    let (
        project_id,
        run_id,
        runtime_kind,
        started_at,
        endpoint,
        heartbeat_at,
        stopped_at,
        next_sequence,
        last_error,
    ) = {
        let mut snapshot = shared.lock().expect("sidecar state lock poisoned");
        snapshot.last_checkpoint_sequence = snapshot.last_checkpoint_sequence.saturating_add(1);
        snapshot.last_checkpoint_at = Some(now_timestamp());
        (
            snapshot.project_id.clone(),
            snapshot.run_id.clone(),
            snapshot.runtime_kind.clone(),
            snapshot.started_at.clone(),
            snapshot.endpoint.clone(),
            snapshot.last_heartbeat_at.clone(),
            snapshot.stopped_at.clone(),
            snapshot.last_checkpoint_sequence,
            snapshot
                .last_error
                .clone()
                .map(protocol_diagnostic_into_record),
        )
    };

    let attempt = {
        let _guard = persistence_lock
            .lock()
            .expect("runtime supervisor persistence lock poisoned");
        project_store::upsert_runtime_run(
            repo_root,
            &RuntimeRunUpsertRecord {
                run: RuntimeRunRecord {
                    project_id: project_id.clone(),
                    run_id: run_id.clone(),
                    runtime_kind: runtime_kind.clone(),
                    supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                    status: status.clone(),
                    transport: RuntimeRunTransportRecord {
                        kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                        endpoint,
                        liveness: RuntimeRunTransportLiveness::Reachable,
                    },
                    started_at,
                    last_heartbeat_at: heartbeat_at,
                    stopped_at,
                    last_error,
                    updated_at: now_timestamp(),
                },
                checkpoint: Some(project_store::RuntimeRunCheckpointRecord {
                    project_id: project_id.clone(),
                    run_id: run_id.clone(),
                    sequence: next_sequence,
                    kind: checkpoint_kind.clone(),
                    summary: summary.clone(),
                    created_at: now_timestamp(),
                }),
            },
        )
    };

    match attempt {
        Ok(snapshot) => Ok(snapshot),
        Err(error)
            if matches!(
                error.code.as_str(),
                "runtime_run_checkpoint_invalid" | "runtime_run_request_invalid"
            ) =>
        {
            let fallback_summary = match checkpoint_kind {
                project_store::RuntimeRunCheckpointKind::ActionRequired => {
                    INTERACTIVE_BOUNDARY_CHECKPOINT_SUMMARY.into()
                }
                _ => REDACTED_SHELL_OUTPUT_SUMMARY.into(),
            };
            let _guard = persistence_lock
                .lock()
                .expect("runtime supervisor persistence lock poisoned");
            project_store::upsert_runtime_run(
                repo_root,
                &RuntimeRunUpsertRecord {
                    run: RuntimeRunRecord {
                        project_id: project_id.clone(),
                        run_id: run_id.clone(),
                        runtime_kind,
                        supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                        status,
                        transport: RuntimeRunTransportRecord {
                            kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                            endpoint: shared
                                .lock()
                                .expect("sidecar state lock poisoned")
                                .endpoint
                                .clone(),
                            liveness: RuntimeRunTransportLiveness::Reachable,
                        },
                        started_at: shared
                            .lock()
                            .expect("sidecar state lock poisoned")
                            .started_at
                            .clone(),
                        last_heartbeat_at: shared
                            .lock()
                            .expect("sidecar state lock poisoned")
                            .last_heartbeat_at
                            .clone(),
                        stopped_at: shared
                            .lock()
                            .expect("sidecar state lock poisoned")
                            .stopped_at
                            .clone(),
                        last_error: shared
                            .lock()
                            .expect("sidecar state lock poisoned")
                            .last_error
                            .clone()
                            .map(protocol_diagnostic_into_record),
                        updated_at: now_timestamp(),
                    },
                    checkpoint: Some(project_store::RuntimeRunCheckpointRecord {
                        project_id,
                        run_id,
                        sequence: next_sequence,
                        kind: checkpoint_kind,
                        summary: fallback_summary,
                        created_at: now_timestamp(),
                    }),
                },
            )
        }
        Err(error) => Err(error),
    }
}

fn persist_runtime_row_from_shared(
    repo_root: &Path,
    shared: &Arc<Mutex<SidecarSharedState>>,
    persistence_lock: &Arc<Mutex<()>>,
) -> Result<RuntimeRunSnapshotRecord, CommandError> {
    let snapshot = shared.lock().expect("sidecar state lock poisoned").clone();
    let _guard = persistence_lock
        .lock()
        .expect("runtime supervisor persistence lock poisoned");
    project_store::upsert_runtime_run(
        repo_root,
        &RuntimeRunUpsertRecord {
            run: RuntimeRunRecord {
                project_id: snapshot.project_id,
                run_id: snapshot.run_id,
                runtime_kind: snapshot.runtime_kind,
                supervisor_kind: SUPERVISOR_KIND_DETACHED_PTY.into(),
                status: match snapshot.status {
                    SupervisorProcessStatus::Starting => RuntimeRunStatus::Starting,
                    SupervisorProcessStatus::Running => RuntimeRunStatus::Running,
                    SupervisorProcessStatus::Stopped => RuntimeRunStatus::Stopped,
                    SupervisorProcessStatus::Failed => RuntimeRunStatus::Failed,
                },
                transport: RuntimeRunTransportRecord {
                    kind: SUPERVISOR_TRANSPORT_KIND_TCP.into(),
                    endpoint: snapshot.endpoint,
                    liveness: RuntimeRunTransportLiveness::Reachable,
                },
                started_at: snapshot.started_at,
                last_heartbeat_at: snapshot.last_heartbeat_at,
                stopped_at: snapshot.stopped_at,
                last_error: snapshot.last_error.map(protocol_diagnostic_into_record),
                updated_at: now_timestamp(),
            },
            checkpoint: None,
        },
    )
}

fn protocol_diagnostic_into_record(
    diagnostic: SupervisorProtocolDiagnostic,
) -> RuntimeRunDiagnosticRecord {
    RuntimeRunDiagnosticRecord {
        code: diagnostic.code,
        message: diagnostic.message,
    }
}

fn summarize_pty_output(raw: &str) -> Option<String> {
    let sanitized = raw
        .chars()
        .map(|character| match character {
            '\n' | '\r' | '\t' => ' ',
            character if character.is_control() => ' ',
            character => character,
        })
        .collect::<String>();
    let collapsed = sanitized.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }

    let bounded = if trimmed.chars().count() > 220 {
        let mut tail = trimmed
            .chars()
            .rev()
            .take(219)
            .collect::<String>()
            .chars()
            .rev()
            .collect::<String>();
        tail.insert(0, '…');
        tail
    } else {
        trimmed.to_string()
    };

    Some(format!("{SHELL_OUTPUT_PREFIX} {bounded}"))
}
