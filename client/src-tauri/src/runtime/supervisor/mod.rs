use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    io::{BufRead, BufReader, Read, Write},
    path::PathBuf,
    sync::{mpsc::SyncSender, Arc, Mutex},
    time::Duration,
};

use serde::{de::DeserializeOwned, Serialize};

use crate::commands::CommandError;

use super::protocol::{
    RuntimeSupervisorLaunchContext, SupervisorControlResponse, SupervisorLiveEventPayload,
    SupervisorProcessStatus, SupervisorProtocolDiagnostic,
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
pub(crate) const CADENCE_RUNTIME_PROVIDER_ID_ENV: &str = "CADENCE_RUNTIME_PROVIDER_ID";
pub(crate) const CADENCE_RUNTIME_SESSION_ID_ENV: &str = "CADENCE_RUNTIME_SESSION_ID";
pub(crate) const CADENCE_RUNTIME_FLOW_ID_ENV: &str = "CADENCE_RUNTIME_FLOW_ID";
pub(crate) const CADENCE_RUNTIME_MODEL_ID_ENV: &str = "CADENCE_RUNTIME_MODEL_ID";
pub(crate) const CADENCE_RUNTIME_THINKING_EFFORT_ENV: &str = "CADENCE_RUNTIME_THINKING_EFFORT";
pub(crate) const ANTHROPIC_API_KEY_ENV: &str = "ANTHROPIC_API_KEY";
pub(crate) const OPENAI_API_KEY_ENV: &str = "OPENAI_API_KEY";
pub(crate) const OPENAI_BASE_URL_ENV: &str = "OPENAI_BASE_URL";
pub(crate) const OPENAI_API_VERSION_ENV: &str = "OPENAI_API_VERSION";

mod boundary;
mod control;
mod host;
mod live_events;
mod persistence;
mod sidecar;

use boundary::ActiveInteractiveBoundary;

pub use host::{
    launch_detached_runtime_supervisor, probe_runtime_run, stop_runtime_run,
    submit_runtime_run_input, update_runtime_run_controls, ActiveRuntimeSupervisorSnapshot,
    RuntimeSupervisorController, RuntimeSupervisorLaunchRequest, RuntimeSupervisorProbeRequest,
    RuntimeSupervisorStopRequest, RuntimeSupervisorSubmitInputRequest,
    RuntimeSupervisorUpdateControlsRequest,
};

#[derive(Clone, Default, PartialEq, Eq)]
pub struct RuntimeSupervisorLaunchEnv {
    vars: BTreeMap<String, String>,
}

impl RuntimeSupervisorLaunchEnv {
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.vars.iter().map(|(key, value)| (key.as_str(), value.as_str()))
    }
}

impl std::fmt::Debug for RuntimeSupervisorLaunchEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let keys = self.vars.keys().cloned().collect::<Vec<_>>();
        f.debug_struct("RuntimeSupervisorLaunchEnv")
            .field("keys", &keys)
            .finish()
    }
}

#[derive(Debug, Clone)]
struct RuntimeSupervisorSidecarArgs {
    project_id: String,
    repo_root: PathBuf,
    runtime_kind: String,
    run_id: String,
    session_id: String,
    flow_id: Option<String>,
    launch_context: RuntimeSupervisorLaunchContext,
    program: String,
    args: Vec<String>,
    run_controls: crate::db::project_store::RuntimeRunControlStateRecord,
}

#[derive(Debug, Clone)]
struct SidecarSharedState {
    project_id: String,
    run_id: String,
    runtime_kind: String,
    provider_id: String,
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
    control_state: crate::db::project_store::RuntimeRunControlStateRecord,
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

fn validate_runtime_supervisor_launch_context(
    runtime_kind: &str,
    session_id: &str,
    flow_id: Option<&str>,
    control_state: &crate::db::project_store::RuntimeRunControlStateRecord,
    launch_context: &RuntimeSupervisorLaunchContext,
) -> Result<(), CommandError> {
    crate::commands::validate_non_empty(&launch_context.provider_id, "launchContext.providerId")?;
    crate::commands::validate_non_empty(&launch_context.session_id, "launchContext.sessionId")?;
    crate::commands::validate_non_empty(&launch_context.model_id, "launchContext.modelId")?;
    if let Some(flow_id) = launch_context.flow_id.as_deref() {
        crate::commands::validate_non_empty(flow_id, "launchContext.flowId")?;
    }

    crate::runtime::resolve_runtime_provider_identity(
        Some(launch_context.provider_id.as_str()),
        Some(runtime_kind),
    )
    .map_err(|diagnostic| {
        CommandError::user_fixable(
            "runtime_supervisor_provider_mismatch",
            format!(
                "Cadence rejected detached runtime launch context because {}",
                diagnostic.message
            ),
        )
    })?;

    if launch_context.session_id != session_id {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_launch_context_invalid",
            "Cadence rejected detached runtime launch context because session identity did not match the requested runtime session.",
        ));
    }

    if launch_context.flow_id.as_deref() != flow_id {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_launch_context_invalid",
            "Cadence rejected detached runtime launch context because flow identity did not match the requested runtime session.",
        ));
    }

    if launch_context.model_id != control_state.active.model_id {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_launch_context_invalid",
            "Cadence rejected detached runtime launch context because model identity did not match the approved control state.",
        ));
    }

    if launch_context.thinking_effort != control_state.active.thinking_effort {
        return Err(CommandError::user_fixable(
            "runtime_supervisor_launch_context_invalid",
            "Cadence rejected detached runtime launch context because thinking effort did not match the approved control state.",
        ));
    }

    Ok(())
}

fn runtime_supervisor_thinking_effort_env_value(
    effort: &crate::commands::ProviderModelThinkingEffortDto,
) -> &'static str {
    match effort {
        crate::commands::ProviderModelThinkingEffortDto::Minimal => "minimal",
        crate::commands::ProviderModelThinkingEffortDto::Low => "low",
        crate::commands::ProviderModelThinkingEffortDto::Medium => "medium",
        crate::commands::ProviderModelThinkingEffortDto::High => "high",
        crate::commands::ProviderModelThinkingEffortDto::XHigh => "xhigh",
    }
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
