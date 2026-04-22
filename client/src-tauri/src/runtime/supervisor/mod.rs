use std::{
    collections::{HashMap, VecDeque},
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    sync::{mpsc::SyncSender, Arc, Mutex},
    time::Duration,
};

use serde::{de::DeserializeOwned, Serialize};

use crate::commands::CommandError;

use super::protocol::{
    SupervisorControlResponse, SupervisorLiveEventPayload, SupervisorProcessStatus,
    SupervisorProtocolDiagnostic,
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
const STRUCTURED_EVENT_PREFIX: &str = "__Cadence_EVENT__ ";
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

mod boundary;
mod control;
mod host;
mod live_events;
mod persistence;
mod sidecar;

use boundary::ActiveInteractiveBoundary;

pub use host::{
    launch_detached_runtime_supervisor, probe_runtime_run, stop_runtime_run,
    submit_runtime_run_input, ActiveRuntimeSupervisorSnapshot, RuntimeSupervisorController,
    RuntimeSupervisorLaunchRequest, RuntimeSupervisorProbeRequest, RuntimeSupervisorStopRequest,
    RuntimeSupervisorSubmitInputRequest,
};

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
    run_controls: crate::db::project_store::RuntimeRunControlStateRecord,
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

pub fn run_supervisor_sidecar_from_env() -> Result<(), CommandError> {
    sidecar::run_supervisor_sidecar_from_env()
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
