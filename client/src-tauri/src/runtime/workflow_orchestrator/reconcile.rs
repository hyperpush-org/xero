use std::{
    collections::{BTreeMap, BTreeSet, HashMap, VecDeque},
    fs::{File, OpenOptions},
    io::{self, Read},
    path::{Path, PathBuf},
    process::{self, Child, ChildStderr, ChildStdout, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, RecvTimeoutError, Sender},
        Arc, Condvar, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::{Duration as StdDuration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{json, Value as JsonValue};
use tauri::{AppHandle, Runtime};
use time::{format_description::well_known::Rfc3339, Duration, OffsetDateTime};

use crate::{
    commands::{
        agent_task::start_agent_task_blocking,
        contracts::{
            runtime::{RuntimeRunApprovalModeDto, RuntimeRunControlInputDto},
            workflow_agents::AgentRefDto,
            workflows::{
                WorkflowCollectionLoopControlsDto, WorkflowConditionDto, WorkflowDefinitionDto,
                WorkflowEdgeDto, WorkflowEdgeTypeDto, WorkflowInputBindingDto,
                WorkflowMergeWaitPolicyDto, WorkflowNodeDto, WorkflowNodeRunStatusDto,
                WorkflowOutputContractDto, WorkflowOutputExtractionDto,
                WorkflowResourceConflictModeDto, WorkflowRunDto, WorkflowRunNodeDto,
                WorkflowRunOverrideDto, WorkflowRunStatusDto, WorkflowStallDetectorDto,
                WorkflowStateQueryDto, WorkflowStateWriteOperationDto, WorkflowSubgraphDto,
                WorkflowTerminalStatusDto,
            },
        },
        default_runtime_agent_approval_mode, CommandError, CommandResult, StartAgentTaskRequestDto,
    },
    db::project_store::{
        self, AgentRunRecord, AgentRunSnapshotRecord, AgentRunStatus, AgentSessionCreateRecord,
    },
    runtime::{
        process_tree::{
            cleanup_process_group_after_root_exit, configure_process_tree_root,
            process_birth_identity, process_identity_is_live, register_process_tree_root,
            terminate_process_tree,
        },
        DesktopAgentCoreRuntime,
    },
    state::DesktopState,
};

use super::{
    artifacts::{
        build_agent_node_prompt, extract_workflow_artifact_payload, final_assistant_text,
        validate_workflow_artifact_payload,
    },
    command_policy::{
        append_workflow_command_arguments, harden_workflow_command_process,
        resolve_workflow_command_executable, validate_workflow_command_policy,
    },
    condition_eval::{
        evaluate_workflow_condition, json_path_lookup, lookup_run_input_binding,
        WorkflowConditionContext,
    },
};

const MAX_RECONCILE_STEPS: usize = 32;
const RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS: &str = "runtime_activity_timeout";
const COMMAND_INTERRUPTED_FAILURE_CLASS: &str = "workflow_command_interrupted";
const SUBGRAPH_NODE_SEPARATOR: &str = "::";
const SUBGRAPH_INPUT_ARTIFACT_TYPE: &str = "subgraph_input";
const MAX_COMMAND_STREAM_CAPTURE_BYTES: usize = 1024 * 1024;
const COMMAND_STREAM_DRAIN_TIMEOUT: StdDuration = StdDuration::from_millis(500);
const COMMAND_TERMINATION_CONFIRM_TIMEOUT: StdDuration = StdDuration::from_secs(10);
const COMMAND_LEASE_HEARTBEAT_INTERVAL: StdDuration = StdDuration::from_secs(2);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RunningCommandKey {
    project_id: String,
    run_id: String,
    node_run_id: String,
}

impl RunningCommandKey {
    fn new(project_id: &str, run_id: &str, node_run_id: &str) -> Self {
        Self {
            project_id: project_id.to_owned(),
            run_id: run_id.to_owned(),
            node_run_id: node_run_id.to_owned(),
        }
    }
}

#[derive(Debug, Default)]
struct RunningCommandControl {
    child: Mutex<Option<Child>>,
    termination_requested: AtomicBool,
}

impl RunningCommandControl {
    fn attach_child(&self, child: Child) {
        let mut slot = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *slot = Some(child);
    }

    fn request_termination(&self) {
        self.termination_requested.store(true, Ordering::Release);
    }

    fn termination_requested(&self) -> bool {
        self.termination_requested.load(Ordering::Acquire)
    }

    fn has_child(&self) -> bool {
        self.child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    fn try_wait(&self) -> io::Result<Option<ExitStatus>> {
        let mut slot = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let child = slot.as_mut().ok_or_else(|| {
            io::Error::other("Workflow command child was not attached to its control handle.")
        })?;
        let child_id = child.id();
        let status = child.try_wait()?;
        if status.is_some() {
            *slot = None;
            drop(slot);
            cleanup_process_group_after_root_exit(child_id);
        }
        Ok(status)
    }

    fn terminate(&self) -> io::Result<ExitStatus> {
        let mut slot = self
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut child = slot.take().ok_or_else(|| {
            io::Error::other("Workflow command child was not attached to its control handle.")
        })?;
        match terminate_process_tree(&mut child) {
            Ok(status) => Ok(status),
            Err(tree_error) => {
                let child_id = child.id();
                match child.try_wait() {
                    Ok(Some(status)) => {
                        cleanup_process_group_after_root_exit(child_id);
                        return Ok(status);
                    }
                    Ok(None) => {}
                    Err(error) => {
                        *slot = Some(child);
                        return Err(error);
                    }
                }
                if let Err(kill_error) = child.kill() {
                    *slot = Some(child);
                    return Err(io::Error::new(
                        kill_error.kind(),
                        format!(
                            "process-tree termination failed ({tree_error}); direct child termination also failed ({kill_error})"
                        ),
                    ));
                }
                match child.wait() {
                    Ok(status) => {
                        cleanup_process_group_after_root_exit(child_id);
                        Ok(status)
                    }
                    Err(error) => {
                        *slot = Some(child);
                        Err(error)
                    }
                }
            }
        }
    }
}

struct RunningCommandRegistration {
    key: RunningCommandKey,
    control: Arc<RunningCommandControl>,
    owner_instance_id: String,
    owner_process_id: u32,
    owner_process_birth_identity: String,
    lease_token: String,
    repo_root: Option<PathBuf>,
    heartbeat_stop: Option<Sender<()>>,
    heartbeat_thread: Option<JoinHandle<()>>,
}

impl RunningCommandRegistration {
    fn activate_persisted_lease(&mut self, repo_root: &Path) -> CommandResult<()> {
        let (heartbeat_stop, stop_receiver) = mpsc::channel();
        let heartbeat_repo_root = repo_root.to_path_buf();
        let heartbeat_project_id = self.key.project_id.clone();
        let heartbeat_node_run_id = self.key.node_run_id.clone();
        let heartbeat_owner_instance_id = self.owner_instance_id.clone();
        let heartbeat_lease_token = self.lease_token.clone();
        let heartbeat_control = self.control.clone();
        let heartbeat_thread = thread::Builder::new()
            .name(format!("workflow-command-lease-{}", self.key.node_run_id))
            .spawn(move || loop {
                match stop_receiver.recv_timeout(COMMAND_LEASE_HEARTBEAT_INTERVAL) {
                    Ok(()) | Err(RecvTimeoutError::Disconnected) => break,
                    Err(RecvTimeoutError::Timeout) => {
                        let renewed = project_store::renew_workflow_command_lease(
                            &heartbeat_repo_root,
                            &heartbeat_project_id,
                            &heartbeat_node_run_id,
                            &heartbeat_owner_instance_id,
                            &heartbeat_lease_token,
                            &crate::auth::now_timestamp(),
                        );
                        if !matches!(renewed, Ok(true)) {
                            heartbeat_control.request_termination();
                            break;
                        }
                    }
                }
            })
            .map_err(|error| {
                CommandError::system_fault(
                    "workflow_command_lease_heartbeat_spawn_failed",
                    format!("Xero could not start the Workflow command lease heartbeat: {error}"),
                )
            })?;
        self.repo_root = Some(repo_root.to_path_buf());
        self.heartbeat_stop = Some(heartbeat_stop);
        self.heartbeat_thread = Some(heartbeat_thread);
        Ok(())
    }
}

impl Drop for RunningCommandRegistration {
    fn drop(&mut self) {
        if let Some(stop) = self.heartbeat_stop.take() {
            let _ = stop.send(());
        }
        self.control.request_termination();
        if self.control.has_child() {
            let _ = self.control.terminate();
        }
        if let Some(worker) = self.heartbeat_thread.take() {
            let _ = worker.join();
        }
        if let Some(repo_root) = self.repo_root.as_ref() {
            let _ = project_store::release_workflow_command_lease(
                repo_root,
                &self.key.project_id,
                &self.key.node_run_id,
                &self.owner_instance_id,
                &self.lease_token,
            );
        }
        let mut commands = running_workflow_commands()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if commands
            .get(&self.key)
            .is_some_and(|registered| Arc::ptr_eq(registered, &self.control))
        {
            commands.remove(&self.key);
        }
    }
}

fn running_workflow_commands(
) -> &'static Mutex<HashMap<RunningCommandKey, Arc<RunningCommandControl>>> {
    static COMMANDS: OnceLock<Mutex<HashMap<RunningCommandKey, Arc<RunningCommandControl>>>> =
        OnceLock::new();
    COMMANDS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_running_workflow_command(
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
) -> CommandResult<RunningCommandRegistration> {
    let key = RunningCommandKey::new(project_id, run_id, node_run_id);
    let control = Arc::new(RunningCommandControl::default());
    let mut commands = running_workflow_commands()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if commands.contains_key(&key) {
        return Err(CommandError::retryable(
            "workflow_command_already_running",
            format!("Workflow command node run `{node_run_id}` is already running."),
        ));
    }
    let owner_process_birth_identity = process_birth_identity(process::id()).ok_or_else(|| {
        CommandError::system_fault(
            "workflow_command_owner_identity_unavailable",
            "Xero could not identify the Workflow command owner process safely.",
        )
    })?;
    commands.insert(key.clone(), control.clone());
    Ok(RunningCommandRegistration {
        key,
        control,
        owner_instance_id: workflow_command_owner_instance_id().to_owned(),
        owner_process_id: process::id(),
        owner_process_birth_identity,
        lease_token: unique_workflow_command_identity("lease"),
        repo_root: None,
        heartbeat_stop: None,
        heartbeat_thread: None,
    })
}

fn workflow_command_owner_instance_id() -> &'static str {
    static INSTANCE_ID: OnceLock<String> = OnceLock::new();
    INSTANCE_ID
        .get_or_init(|| unique_workflow_command_identity("app"))
        .as_str()
}

fn unique_workflow_command_identity(prefix: &str) -> String {
    static NEXT_ID: AtomicU64 = AtomicU64::new(1);
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!("{prefix}-{}-{nanos:x}-{sequence:x}", process::id())
}

fn running_workflow_command_is_registered(
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
) -> bool {
    let key = RunningCommandKey::new(project_id, run_id, node_run_id);
    running_workflow_commands()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .contains_key(&key)
}

/// Requests termination without waiting for the command thread. The command
/// thread owns the child handle and performs process-tree cleanup, allowing
/// cancellation and branch controls to commit durable state immediately.
pub(crate) fn terminate_running_workflow_commands(
    project_id: &str,
    run_id: &str,
    node_run_id: Option<&str>,
) -> usize {
    let controls = running_workflow_commands()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .iter()
        .filter(|(key, _)| {
            key.project_id == project_id
                && key.run_id == run_id
                && node_run_id.is_none_or(|node_run_id| key.node_run_id == node_run_id)
        })
        .map(|(_, control)| control.clone())
        .collect::<Vec<_>>();
    for control in &controls {
        control.request_termination();
    }
    controls.len()
}

pub(crate) fn shutdown_running_workflow_commands() -> usize {
    let controls = running_workflow_commands()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .values()
        .cloned()
        .collect::<Vec<_>>();
    for control in &controls {
        control.request_termination();
        if control.has_child() {
            let _ = control.terminate();
        }
    }
    controls.len()
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BoundedCommandCapture {
    bytes: Vec<u8>,
    truncated: bool,
    drain_incomplete: bool,
    read_error: Option<String>,
}

fn read_bounded_command_stream(
    mut reader: impl Read,
    max_bytes: usize,
) -> io::Result<BoundedCommandCapture> {
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let remaining = max_bytes.saturating_sub(bytes.len());
        let retained = remaining.min(read);
        bytes.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
    Ok(BoundedCommandCapture {
        bytes,
        truncated,
        drain_incomplete: false,
        read_error: None,
    })
}

#[derive(Debug)]
struct BoundedCommandReaderState {
    capture: BoundedCommandCapture,
    completed: bool,
}

struct BoundedCommandReader {
    state: Arc<(Mutex<BoundedCommandReaderState>, Condvar)>,
    stop: Arc<AtomicBool>,
    thread: JoinHandle<()>,
}

trait PollableCommandStream: Read + Send + 'static {
    fn wait_until_readable(&self, timeout: StdDuration) -> io::Result<bool>;
}

#[cfg(unix)]
fn unix_command_stream_readable(
    stream: &impl std::os::fd::AsRawFd,
    timeout: StdDuration,
) -> io::Result<bool> {
    let mut descriptor = libc::pollfd {
        fd: stream.as_raw_fd(),
        events: libc::POLLIN | libc::POLLHUP | libc::POLLERR,
        revents: 0,
    };
    loop {
        let result = unsafe {
            libc::poll(
                &mut descriptor,
                1,
                timeout.as_millis().min(i32::MAX as u128) as i32,
            )
        };
        if result >= 0 {
            return Ok(result > 0);
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}

#[cfg(windows)]
fn windows_command_stream_readable(
    stream: &impl std::os::windows::io::AsRawHandle,
    timeout: StdDuration,
) -> io::Result<bool> {
    use std::ffi::c_void;

    #[link(name = "kernel32")]
    unsafe extern "system" {
        #[link_name = "PeekNamedPipe"]
        fn peek_named_pipe(
            pipe: *mut c_void,
            buffer: *mut c_void,
            buffer_size: u32,
            bytes_read: *mut u32,
            total_bytes_available: *mut u32,
            bytes_left_this_message: *mut u32,
        ) -> i32;
    }

    const ERROR_BROKEN_PIPE: i32 = 109;
    let deadline = Instant::now() + timeout;
    loop {
        let mut available = 0_u32;
        let result = unsafe {
            peek_named_pipe(
                stream.as_raw_handle().cast(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if result != 0 {
            if available > 0 {
                return Ok(true);
            }
        } else {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(ERROR_BROKEN_PIPE) {
                return Ok(true);
            }
            return Err(error);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        thread::sleep(StdDuration::from_millis(5));
    }
}

impl PollableCommandStream for ChildStdout {
    fn wait_until_readable(&self, timeout: StdDuration) -> io::Result<bool> {
        #[cfg(unix)]
        {
            unix_command_stream_readable(self, timeout)
        }
        #[cfg(windows)]
        {
            windows_command_stream_readable(self, timeout)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = timeout;
            Ok(true)
        }
    }
}

impl PollableCommandStream for ChildStderr {
    fn wait_until_readable(&self, timeout: StdDuration) -> io::Result<bool> {
        #[cfg(unix)]
        {
            unix_command_stream_readable(self, timeout)
        }
        #[cfg(windows)]
        {
            windows_command_stream_readable(self, timeout)
        }
        #[cfg(not(any(unix, windows)))]
        {
            let _ = timeout;
            Ok(true)
        }
    }
}

#[cfg(test)]
impl<T> PollableCommandStream for std::io::Cursor<T>
where
    std::io::Cursor<T>: Read + Send + 'static,
{
    fn wait_until_readable(&self, _timeout: StdDuration) -> io::Result<bool> {
        Ok(true)
    }
}

fn spawn_bounded_command_reader(mut reader: impl PollableCommandStream) -> BoundedCommandReader {
    let state = Arc::new((
        Mutex::new(BoundedCommandReaderState {
            capture: BoundedCommandCapture {
                bytes: Vec::with_capacity(MAX_COMMAND_STREAM_CAPTURE_BYTES.min(8192)),
                truncated: false,
                drain_incomplete: false,
                read_error: None,
            },
            completed: false,
        }),
        Condvar::new(),
    ));
    let reader_state = state.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let reader_stop = stop.clone();
    let thread = thread::spawn(move || {
        let mut buffer = [0_u8; 8192];
        loop {
            if reader_stop.load(Ordering::Acquire) {
                let (state, completed) = &*reader_state;
                let mut state = state
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                state.completed = true;
                completed.notify_all();
                return;
            }
            match reader.wait_until_readable(StdDuration::from_millis(25)) {
                Ok(true) => {}
                Ok(false) => continue,
                Err(error) => {
                    let (state, completed) = &*reader_state;
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.capture.read_error = Some(error.to_string());
                    state.completed = true;
                    completed.notify_all();
                    return;
                }
            }
            match reader.read(&mut buffer) {
                Ok(0) => {
                    let (state, completed) = &*reader_state;
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.completed = true;
                    completed.notify_all();
                    return;
                }
                Ok(read) => {
                    let (state, _) = &*reader_state;
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let remaining =
                        MAX_COMMAND_STREAM_CAPTURE_BYTES.saturating_sub(state.capture.bytes.len());
                    let retained = remaining.min(read);
                    state.capture.bytes.extend_from_slice(&buffer[..retained]);
                    state.capture.truncated |= retained < read;
                }
                Err(error) => {
                    let (state, completed) = &*reader_state;
                    let mut state = state
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    state.capture.read_error = Some(error.to_string());
                    state.completed = true;
                    completed.notify_all();
                    return;
                }
            }
        }
    });
    BoundedCommandReader {
        state,
        stop,
        thread,
    }
}

fn finish_bounded_command_reader(
    reader: BoundedCommandReader,
    deadline: Instant,
) -> BoundedCommandCapture {
    let BoundedCommandReader {
        state,
        stop,
        thread,
    } = reader;
    let (capture_state, completed) = &*state;
    let mut capture_guard = capture_state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    while !capture_guard.completed {
        let now = Instant::now();
        if now >= deadline {
            capture_guard.capture.truncated = true;
            capture_guard.capture.drain_incomplete = true;
            break;
        }
        let (next_state, wait_result) = completed
            .wait_timeout(capture_guard, deadline.saturating_duration_since(now))
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        capture_guard = next_state;
        if wait_result.timed_out() && !capture_guard.completed {
            capture_guard.capture.truncated = true;
            capture_guard.capture.drain_incomplete = true;
            break;
        }
    }
    drop(capture_guard);
    stop.store(true, Ordering::Release);
    let reader_panicked = thread.join().is_err();
    let mut capture = capture_state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .capture
        .clone();
    if reader_panicked {
        capture.truncated = true;
        capture.drain_incomplete = true;
        capture.read_error = Some("command stream reader panicked".into());
    }
    capture
}

pub fn reconcile_workflow_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    for _ in 0..MAX_RECONCILE_STEPS {
        let run =
            project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
                CommandError::user_fixable(
                    "workflow_run_not_found",
                    format!("Xero could not find Workflow run `{run_id}`."),
                )
            })?;
        if project_store::workflow_run_cancellation_pending(&repo_root, project_id, run_id)? {
            complete_pending_workflow_cancellation(state, &repo_root, project_id, &run)?;
            continue;
        }
        if is_terminal_run(run.status) || run.status == WorkflowRunStatusDto::Paused {
            return Ok(run);
        }

        if run.status == WorkflowRunStatusDto::Queued
            || (run.status == WorkflowRunStatusDto::Running && run.nodes.is_empty())
        {
            let start_node_id = run.definition_snapshot.start_node_id.as_str();
            let start_node_type = find_node(&run.definition_snapshot, start_node_id)
                .map(|node| node.node_type().as_str())
                .ok_or_else(|| {
                    CommandError::system_fault(
                        "workflow_start_node_missing",
                        format!("Workflow start node `{start_node_id}` is missing."),
                    )
                })?;
            project_store::start_workflow_run_atomically(
                &repo_root,
                project_id,
                run_id,
                start_node_id,
                start_node_type,
            )?;
            continue;
        }

        if complete_ready_top_level_terminal(state, &repo_root, project_id, &run)? {
            continue;
        }

        if reconcile_interrupted_command_nodes(&repo_root, project_id, &run)? {
            continue;
        }

        if reconcile_starting_agent_nodes(app, state, &repo_root, project_id, &run)? {
            continue;
        }

        if reconcile_running_agent_nodes(state, &repo_root, project_id, &run)? {
            continue;
        }

        if route_completed_nodes(state, &repo_root, project_id, &run)? {
            continue;
        }

        if start_eligible_nodes(app, state, &repo_root, project_id, &run)? {
            continue;
        }

        return project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::system_fault(
                "workflow_run_missing_after_reconcile",
                format!("Workflow run `{run_id}` disappeared during reconcile."),
            )
        });
    }

    project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "workflow_run_missing_after_reconcile",
            format!("Workflow run `{run_id}` disappeared during reconcile."),
        )
    })
}

fn complete_ready_top_level_terminal(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    let ready = run.nodes.iter().find_map(|node_run| {
        matches!(
            node_run.status,
            WorkflowNodeRunStatusDto::Eligible | WorkflowNodeRunStatusDto::Succeeded
        )
        .then(|| find_node(&run.definition_snapshot, &node_run.node_id))
        .flatten()
        .and_then(|node| match node {
            WorkflowNodeDto::Terminal {
                terminal_status, ..
            } if subgraph_context_for_node_id(&run.definition_snapshot, &node_run.node_id)
                .is_none() =>
            {
                Some((node_run, *terminal_status))
            }
            _ => None,
        })
    });
    let Some((node_run, terminal_status)) = ready else {
        return Ok(false);
    };
    complete_for_terminal(state, repo_root, project_id, run, node_run, terminal_status)?;
    Ok(true)
}

fn complete_pending_workflow_cancellation(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<()> {
    for node_run in &run.nodes {
        if matches!(
            find_node(&run.definition_snapshot, &node_run.node_id),
            Some(WorkflowNodeDto::Agent { .. } | WorkflowNodeDto::Command { .. })
        ) {
            terminate_workflow_node_execution(state, repo_root, project_id, run, node_run)?;
        }
    }
    project_store::cancel_workflow_run_execution(
        repo_root,
        project_id,
        &run.id,
        run.cancellation_reason.as_deref(),
    )?;
    Ok(())
}

fn reconcile_starting_agent_nodes<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    for node_run in run
        .nodes
        .iter()
        .filter(|node| node.status == WorkflowNodeRunStatusDto::Starting)
    {
        let Some(WorkflowNodeDto::Agent {
            title,
            agent_ref,
            input_bindings,
            run_overrides,
            ..
        }) = find_node(&run.definition_snapshot, &node_run.node_id)
        else {
            continue;
        };
        let input_bindings = runtime_input_bindings_for_node(
            &run.definition_snapshot,
            &node_run.node_id,
            input_bindings,
        );
        start_agent_node(
            app,
            state,
            repo_root,
            project_id,
            run,
            node_run,
            title,
            agent_ref,
            &input_bindings,
            run_overrides.as_ref(),
        )?;
        return Ok(true);
    }
    Ok(false)
}

pub fn resume_workflow_checkpoint<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    decision: &str,
    payload: Option<JsonValue>,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    let run =
        project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_run_not_found",
                format!("Xero could not find Workflow run `{run_id}`."),
            )
        })?;
    let node_run = run
        .nodes
        .iter()
        .find(|node| node.id == node_run_id)
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_checkpoint_not_found",
                format!("Xero could not find Workflow checkpoint node run `{node_run_id}`."),
            )
        })?;
    let Some(WorkflowNodeDto::HumanCheckpoint {
        checkpoint_type,
        decision_options,
        resume_payload_schema,
        state_updates,
        ..
    }) = find_node(&run.definition_snapshot, &node_run.node_id)
    else {
        return Err(CommandError::user_fixable(
            "workflow_checkpoint_node_invalid",
            format!(
                "Workflow node `{}` is not a human checkpoint and cannot be resumed.",
                node_run.node_id
            ),
        ));
    };
    if decision.trim().is_empty()
        || (!decision_options.is_empty()
            && !decision_options.iter().any(|option| option == decision))
    {
        return Err(CommandError::user_fixable(
            "workflow_checkpoint_decision_invalid",
            format!(
                "Decision `{decision}` is not allowed for Workflow checkpoint `{}`.",
                node_run.node_id
            ),
        ));
    }
    if let Some(schema) = resume_payload_schema.as_ref() {
        let payload_value = payload.as_ref().unwrap_or(&JsonValue::Null);
        validate_workflow_artifact_payload(
            &WorkflowOutputContractDto {
                artifact_type: "human_decision".into(),
                schema_version: 1,
                extraction: WorkflowOutputExtractionDto::JsonObject,
                required: true,
                render_text_path: None,
            },
            Some(schema),
            payload_value,
        )
        .map_err(|error| {
            CommandError::user_fixable(
                "workflow_checkpoint_payload_invalid",
                format!(
                    "Xero rejected the resume payload for checkpoint `{}`: {}",
                    node_run.node_id, error.message
                ),
            )
        })?;
    }
    if let Some(existing) = run
        .gate_decisions
        .iter()
        .find(|existing| existing.node_run_id == node_run.id)
    {
        let resume_committed = run.events.iter().any(|event| {
            event.node_run_id.as_deref() == Some(node_run.id.as_str())
                && event.event_type == "workflow_checkpoint_resumed"
        });
        if resume_committed
            && existing.checkpoint_type == *checkpoint_type
            && existing.decision == decision
            && existing.decision_payload == payload
        {
            return Ok(run);
        }
        return Err(CommandError::user_fixable(
            "workflow_checkpoint_already_resumed",
            format!(
                "Workflow checkpoint `{}` was already resumed with a different decision or payload.",
                node_run.id
            ),
        ));
    }
    if run.status != WorkflowRunStatusDto::Paused
        || node_run.status != WorkflowNodeRunStatusDto::WaitingOnGate
    {
        return Err(CommandError::user_fixable(
            "workflow_checkpoint_not_waiting",
            format!("Workflow checkpoint `{node_run_id}` is not waiting in a paused Workflow run."),
        ));
    }
    let decision_context = json!({ "decision": decision, "payload": payload.clone() });
    let resolved_state_updates = state_updates
        .iter()
        .map(|operation| {
            let operation = runtime_state_write_operation_for_node(
                &run.definition_snapshot,
                &node_run.node_id,
                operation,
            );
            resolve_state_write_operation(&operation, &run, Some(&decision_context))
        })
        .collect::<CommandResult<Vec<_>>>()?;
    let _committed = project_store::resume_workflow_checkpoint_atomically(
        &repo_root,
        project_id,
        &project_store::WorkflowCheckpointResumeRecord {
            run_id: run.id.clone(),
            node_run_id: node_run.id.clone(),
            checkpoint_type: *checkpoint_type,
            decision: decision.into(),
            payload,
            state_updates: resolved_state_updates,
        },
    )?;
    project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "workflow_run_missing_after_checkpoint_resume",
            format!("Workflow run `{run_id}` disappeared after its checkpoint was resumed."),
        )
    })
}

pub fn retry_workflow_node_run<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    let run =
        project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_run_not_found",
                format!("Xero could not find Workflow run `{run_id}`."),
            )
        })?;
    if matches!(
        run.status,
        WorkflowRunStatusDto::Completed
            | WorkflowRunStatusDto::Cancelling
            | WorkflowRunStatusDto::Cancelled
    ) {
        return Err(CommandError::user_fixable(
            "workflow_run_not_retryable",
            "Completed, cancelling, or cancelled Workflow runs cannot be retried from a node.",
        ));
    }
    let node_run = run
        .nodes
        .iter()
        .find(|node| node.id == node_run_id)
        .cloned()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_node_run_not_found",
                format!("Xero could not find Workflow node run `{node_run_id}`."),
            )
        })?;
    if !is_retryable_node_status(node_run.status) {
        return Err(CommandError::user_fixable(
            "workflow_node_run_not_retryable",
            format!(
                "Workflow node run `{node_run_id}` cannot be retried while it is `{}`.",
                node_run.status.as_str()
            ),
        ));
    }
    if find_node(&run.definition_snapshot, &node_run.node_id).is_none() {
        return Err(CommandError::system_fault(
            "workflow_retry_node_missing",
            format!(
                "Workflow node `{}` was missing from run `{run_id}`.",
                node_run.node_id
            ),
        ));
    }
    for candidate in &run.nodes {
        if !matches!(
            find_node(&run.definition_snapshot, &candidate.node_id),
            Some(WorkflowNodeDto::Agent { .. })
        ) {
            continue;
        }
        let snapshot = if let Some(runtime_run_id) = candidate.runtime_run_id.as_deref() {
            Some(project_store::load_agent_run(
                &repo_root,
                project_id,
                runtime_run_id,
            )?)
        } else {
            load_existing_agent_run_for_node(&repo_root, project_id, candidate)?
        };
        let Some(snapshot) = snapshot else {
            continue;
        };
        let AgentHandoffLeafResolution::Leaf(leaf) =
            resolve_agent_handoff_leaf(&repo_root, project_id, snapshot, false)?
        else {
            return Err(CommandError::retryable(
                "workflow_retry_handoff_unresolved",
                "The Workflow cannot retry while an owned-agent handoff has unresolved lineage.",
            ));
        };
        if matches!(
            leaf.run.status,
            AgentRunStatus::Starting
                | AgentRunStatus::Running
                | AgentRunStatus::Paused
                | AgentRunStatus::Cancelling
        ) {
            return Err(CommandError::user_fixable(
                "workflow_retry_execution_still_active",
                format!(
                    "Owned-agent run `{}` is still active, so the Workflow cannot rewind for retry.",
                    leaf.run.run_id
                ),
            ));
        }
    }
    if workflow_node_execution_is_live(state, &repo_root, project_id, &run, &node_run)? {
        return Err(CommandError::user_fixable(
            "workflow_node_execution_still_active",
            format!(
                "Workflow node run `{node_run_id}` still owns a live execution and cannot be retried yet."
            ),
        ));
    }
    let node_type = find_node(&run.definition_snapshot, &node_run.node_id)
        .map(|node| node.node_type().as_str().to_owned())
        .ok_or_else(|| {
            CommandError::system_fault(
                "workflow_retry_node_missing",
                format!("Workflow node `{}` is missing.", node_run.node_id),
            )
        })?;
    project_store::retry_workflow_node_atomically(
        &repo_root,
        project_id,
        &project_store::WorkflowNodeRetryRecord {
            run_id: run.id.clone(),
            source_node_run_id: node_run.id.clone(),
            node_id: node_run.node_id.clone(),
            node_type,
        },
    )?;
    project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "workflow_run_missing_after_retry",
            format!("Workflow run `{run_id}` disappeared after retry was committed."),
        )
    })
}

pub fn skip_workflow_branch<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    project_id: &str,
    run_id: &str,
    node_run_id: &str,
    reason: Option<&str>,
) -> CommandResult<WorkflowRunDto> {
    let repo_root = crate::commands::runtime_support::resolve_project_root(app, state, project_id)?;
    let run =
        project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_run_not_found",
                format!("Xero could not find Workflow run `{run_id}`."),
            )
        })?;
    if is_terminal_run(run.status) || run.status == WorkflowRunStatusDto::Cancelling {
        return Err(CommandError::user_fixable(
            "workflow_run_not_skippable",
            "Completed, failed, cancelling, or cancelled Workflow runs cannot skip branches.",
        ));
    }
    let node_run = run
        .nodes
        .iter()
        .find(|node| node.id == node_run_id)
        .cloned()
        .ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_node_run_not_found",
                format!("Xero could not find Workflow node run `{node_run_id}`."),
            )
        })?;
    if node_run.status == WorkflowNodeRunStatusDto::Skipped
        && run.events.iter().any(|event| {
            event.node_run_id.as_deref() == Some(node_run.id.as_str())
                && event.event_type == "workflow_branch_skipped"
        })
    {
        return Ok(run);
    }
    if !is_skippable_node_status(node_run.status) {
        return Err(CommandError::user_fixable(
            "workflow_node_run_not_skippable",
            format!(
                "Workflow node run `{node_run_id}` cannot be skipped while it is `{}`.",
                node_run.status.as_str()
            ),
        ));
    }

    let merge_targets = direct_merge_targets_for_skipped_branch(&run, &node_run.node_id)?;
    terminate_workflow_node_execution(state, &repo_root, project_id, &run, &node_run)?;
    project_store::skip_workflow_branch_atomically(
        &repo_root,
        project_id,
        &project_store::WorkflowBranchSkipRecord {
            run_id: run.id.clone(),
            node_run_id: node_run.id.clone(),
            node_id: node_run.node_id.clone(),
            previous_status: node_run.status,
            reason: reason.map(ToOwned::to_owned),
            merge_targets,
        },
    )?;
    project_store::get_workflow_run(&repo_root, project_id, run_id)?.ok_or_else(|| {
        CommandError::system_fault(
            "workflow_run_missing_after_skip",
            format!("Workflow run `{run_id}` disappeared after its branch was skipped."),
        )
    })
}

fn reconcile_running_agent_nodes(
    state: &DesktopState,
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    let mut changed = false;
    let now = OffsetDateTime::now_utc();
    for node_run in run.nodes.iter().filter(|node| {
        matches!(
            node.status,
            WorkflowNodeRunStatusDto::Running | WorkflowNodeRunStatusDto::WaitingOnGate
        )
    }) {
        let Some(runtime_run_id) = node_run.runtime_run_id.as_deref() else {
            continue;
        };
        let source_snapshot = project_store::load_agent_run(repo_root, project_id, runtime_run_id)?;
        let snapshot =
            match resolve_agent_handoff_leaf(repo_root, project_id, source_snapshot, true)? {
                AgentHandoffLeafResolution::Leaf(snapshot) => snapshot,
                AgentHandoffLeafResolution::Incomplete(reason) => {
                    fail_node_with_recoverable_error(
                        repo_root,
                        project_id,
                        run,
                        node_run,
                        "workflow_agent_handoff_incomplete",
                        "workflow_agent_handoff_incomplete",
                        &reason,
                    )?;
                    changed = true;
                    continue;
                }
            };
        let runtime_run_id = snapshot.run.run_id.as_str();
        if node_run.runtime_run_id.as_deref() != Some(runtime_run_id) {
            if !project_store::compare_and_set_workflow_run_node(
                repo_root,
                project_id,
                &node_run.id,
                &[node_run.status],
                node_run.status,
                Some(runtime_run_id),
                Some(&snapshot.run.agent_session_id),
                None,
            )? {
                continue;
            }
            project_store::insert_workflow_event(
                repo_root,
                project_id,
                &run.id,
                Some(&node_run.id),
                "workflow_agent_handoff_followed",
                &json!({
                    "sourceRuntimeRunId": node_run.runtime_run_id,
                    "targetRuntimeRunId": runtime_run_id,
                    "targetAgentSessionId": snapshot.run.agent_session_id,
                }),
            )?;
            changed = true;
        }
        match snapshot.run.status {
            AgentRunStatus::Starting | AgentRunStatus::Running => {
                if node_run.status == WorkflowNodeRunStatusDto::WaitingOnGate {
                    if project_store::compare_and_set_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        &[WorkflowNodeRunStatusDto::WaitingOnGate],
                        WorkflowNodeRunStatusDto::Running,
                        Some(runtime_run_id),
                        Some(&snapshot.run.agent_session_id),
                        None,
                    )? {
                        project_store::insert_workflow_event(
                            repo_root,
                            project_id,
                            &run.id,
                            Some(&node_run.id),
                            "workflow_agent_resumed",
                            &json!({
                                "nodeId": node_run.node_id,
                                "runtimeRunId": runtime_run_id,
                                "agentSessionId": snapshot.run.agent_session_id,
                            }),
                        )?;
                        changed = true;
                    }
                    continue;
                }
                if let Some(timeout_seconds) =
                    activity_timeout_seconds_for_node(&run.definition_snapshot, &node_run.node_id)
                {
                    if let Some(last_activity_at) =
                        stale_agent_activity_at(&snapshot.run, timeout_seconds, now)
                    {
                        let runtime =
                            DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
                        let cancelled = match runtime.cancel_run(
                            repo_root.to_path_buf(),
                            project_id.to_owned(),
                            runtime_run_id.to_owned(),
                        ) {
                            Ok(snapshot) => snapshot,
                            Err(error) if error.code == "agent_run_active_in_another_process" => {
                                continue;
                            }
                            Err(error) => return Err(error),
                        };
                        if !matches!(
                            cancelled.run.status,
                            AgentRunStatus::Cancelled
                                | AgentRunStatus::HandedOff
                                | AgentRunStatus::Completed
                                | AgentRunStatus::Failed
                        ) {
                            return Err(CommandError::retryable(
                                "workflow_activity_timeout_cancel_incomplete",
                                format!(
                                    "Owned-agent run `{runtime_run_id}` did not reach a terminal state after activity timeout cancellation."
                                ),
                            ));
                        }
                        if !project_store::stall_workflow_agent_for_activity_timeout(
                            repo_root,
                            project_id,
                            &run.id,
                            &node_run.id,
                            &json!({
                                "nodeId": node_run.node_id,
                                "runtimeRunId": runtime_run_id,
                                "failureClass": RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS,
                                "timeoutSeconds": timeout_seconds,
                                "lastActivityAt": last_activity_at,
                                "runtimeTerminalStatus": format!("{:?}", cancelled.run.status).to_lowercase(),
                            }),
                        )? {
                            continue;
                        }
                        changed = true;
                    }
                }
            }
            AgentRunStatus::Paused => {
                if node_run.status == WorkflowNodeRunStatusDto::Running
                    && project_store::compare_and_set_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        &[WorkflowNodeRunStatusDto::Running],
                        WorkflowNodeRunStatusDto::WaitingOnGate,
                        Some(runtime_run_id),
                        Some(&snapshot.run.agent_session_id),
                        None,
                    )?
                {
                    project_store::insert_workflow_event(
                        repo_root,
                        project_id,
                        &run.id,
                        Some(&node_run.id),
                        "workflow_agent_paused",
                        &json!({
                            "nodeId": node_run.node_id,
                            "runtimeRunId": runtime_run_id,
                            "agentSessionId": snapshot.run.agent_session_id,
                            "error": snapshot.run.last_error,
                        }),
                    )?;
                    changed = true;
                }
            }
            AgentRunStatus::Completed => {
                if let Some(contract) =
                    output_contract_for_node(&run.definition_snapshot, &node_run.node_id)
                {
                    let final_text = final_assistant_text(&snapshot).unwrap_or_default();
                    let json_schema =
                        artifact_schema_for_output(&run.definition_snapshot, contract);
                    let (payload, render_text, diagnostics) =
                        match extract_workflow_artifact_payload(contract, json_schema, &final_text)
                        {
                            Ok(artifact) => artifact,
                            Err(error)
                                if error.code == "workflow_artifact_extraction_failed"
                                    || error.code == "workflow_artifact_schema_invalid" =>
                            {
                                fail_node_with_recoverable_error(
                                    repo_root,
                                    project_id,
                                    run,
                                    node_run,
                                    &error.code,
                                    &error.code,
                                    &error.message,
                                )?;
                                changed = true;
                                continue;
                            }
                            Err(error) => return Err(error),
                        };
                    let event = json!({
                        "nodeId": node_run.node_id,
                        "artifactType": contract.artifact_type.as_str(),
                        "schemaVersion": contract.schema_version,
                        "validationStatus": "valid",
                        "diagnostics": diagnostics.iter().map(|diagnostic| {
                            json!({
                                "code": diagnostic.code,
                                "path": diagnostic.path,
                                "message": diagnostic.message,
                            })
                        }).collect::<Vec<_>>(),
                        "renderTextPath": contract.render_text_path.as_deref(),
                    });
                    if !project_store::complete_workflow_agent_node_with_artifact(
                        repo_root,
                        project_id,
                        &project_store::WorkflowAgentArtifactCompletionRecord {
                            run_id: run.id.clone(),
                            node_run_id: node_run.id.clone(),
                            artifact_type: contract.artifact_type.clone(),
                            schema_version: contract.schema_version,
                            payload,
                            render_text,
                            event,
                        },
                    )? {
                        continue;
                    }
                } else if !project_store::compare_and_set_workflow_run_node(
                    repo_root,
                    project_id,
                    &node_run.id,
                    &[
                        WorkflowNodeRunStatusDto::Running,
                        WorkflowNodeRunStatusDto::WaitingOnGate,
                    ],
                    WorkflowNodeRunStatusDto::Succeeded,
                    None,
                    None,
                    None,
                )? {
                    continue;
                }
                changed = true;
            }
            AgentRunStatus::Failed => {
                let failure_class = snapshot
                    .run
                    .last_error
                    .as_ref()
                    .map(|error| error.code.as_str())
                    .unwrap_or("agent_failed");
                if project_store::compare_and_set_workflow_run_node(
                    repo_root,
                    project_id,
                    &node_run.id,
                    &[
                        WorkflowNodeRunStatusDto::Running,
                        WorkflowNodeRunStatusDto::WaitingOnGate,
                    ],
                    WorkflowNodeRunStatusDto::Failed,
                    None,
                    None,
                    Some(failure_class),
                )? {
                    changed = true;
                }
            }
            AgentRunStatus::Cancelled => {
                if project_store::compare_and_set_workflow_run_node(
                    repo_root,
                    project_id,
                    &node_run.id,
                    &[
                        WorkflowNodeRunStatusDto::Running,
                        WorkflowNodeRunStatusDto::WaitingOnGate,
                    ],
                    WorkflowNodeRunStatusDto::Cancelled,
                    None,
                    None,
                    Some("cancelled"),
                )? {
                    changed = true;
                }
            }
            _ => {}
        }
    }
    Ok(changed)
}

enum AgentHandoffLeafResolution {
    Leaf(AgentRunSnapshotRecord),
    Incomplete(String),
}

fn resolve_agent_handoff_leaf(
    repo_root: &Path,
    project_id: &str,
    mut snapshot: AgentRunSnapshotRecord,
    require_completed_lineage: bool,
) -> CommandResult<AgentHandoffLeafResolution> {
    const MAX_HANDOFF_DEPTH: usize = 32;
    let mut visited = BTreeSet::new();

    for _ in 0..MAX_HANDOFF_DEPTH {
        if snapshot.run.status != AgentRunStatus::HandedOff {
            return Ok(AgentHandoffLeafResolution::Leaf(snapshot));
        }
        if !visited.insert(snapshot.run.run_id.clone()) {
            return Ok(AgentHandoffLeafResolution::Incomplete(format!(
                "Owned-agent handoff lineage contains a cycle at run `{}`.",
                snapshot.run.run_id
            )));
        }
        let lineage = project_store::list_agent_handoff_lineage_for_source(
            repo_root,
            project_id,
            &snapshot.run.run_id,
        )?
        .into_iter()
        .next();
        let Some(lineage) = lineage else {
            return Ok(AgentHandoffLeafResolution::Incomplete(format!(
                "Owned-agent run `{}` was handed off without durable lineage.",
                snapshot.run.run_id
            )));
        };
        if require_completed_lineage
            && lineage.status != project_store::AgentHandoffLineageStatus::Completed
        {
            return Ok(AgentHandoffLeafResolution::Incomplete(format!(
                "Owned-agent handoff `{}` is `{}` instead of completed.",
                lineage.handoff_id,
                format!("{:?}", lineage.status).to_lowercase()
            )));
        }
        let Some(target_run_id) = lineage.target_run_id else {
            return Ok(AgentHandoffLeafResolution::Incomplete(format!(
                "Owned-agent handoff `{}` has no target run.",
                lineage.handoff_id
            )));
        };
        snapshot = project_store::load_agent_run(repo_root, project_id, &target_run_id)?;
    }

    Ok(AgentHandoffLeafResolution::Incomplete(format!(
        "Owned-agent handoff lineage exceeded {MAX_HANDOFF_DEPTH} links."
    )))
}

fn activity_timeout_seconds_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
) -> Option<u32> {
    match find_node(definition, node_id) {
        Some(WorkflowNodeDto::Agent { failure_policy, .. }) => failure_policy
            .runtime_activity_timeout_seconds
            .or(definition.run_policy.node_timeout_seconds),
        _ => None,
    }
}

fn stale_agent_activity_at(
    agent_run: &AgentRunRecord,
    timeout_seconds: u32,
    now: OffsetDateTime,
) -> Option<&str> {
    let timeout = Duration::seconds(timeout_seconds.into());
    let latest_activity = [
        agent_run.last_heartbeat_at.as_deref(),
        Some(agent_run.updated_at.as_str()),
        Some(agent_run.started_at.as_str()),
    ]
    .into_iter()
    .flatten()
    .filter_map(|timestamp| {
        OffsetDateTime::parse(timestamp, &Rfc3339)
            .ok()
            .map(|parsed| (timestamp, parsed))
    })
    .max_by_key(|(_, timestamp)| timestamp.unix_timestamp_nanos())?;

    (now - latest_activity.1 >= timeout).then_some(latest_activity.0)
}

fn route_completed_nodes(
    state: &DesktopState,
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    for node_run in run.nodes.iter().filter(|node| {
        matches!(
            node.status,
            WorkflowNodeRunStatusDto::Succeeded
                | WorkflowNodeRunStatusDto::Failed
                | WorkflowNodeRunStatusDto::Stalled
                | WorkflowNodeRunStatusDto::Cancelled
        ) && !has_routed_node_run(run, node)
    }) {
        if workflow_node_execution_is_live(state, repo_root, project_id, run, node_run)? {
            continue;
        }
        let Some(node) = find_node(&run.definition_snapshot, &node_run.node_id) else {
            continue;
        };
        if matches!(node, WorkflowNodeDto::Terminal { .. })
            && subgraph_context_for_node_id(&run.definition_snapshot, &node_run.node_id).is_some()
        {
            continue;
        }
        if let WorkflowNodeDto::Terminal {
            terminal_status, ..
        } = node
        {
            complete_for_terminal(
                state,
                repo_root,
                project_id,
                run,
                node_run,
                *terminal_status,
            )?;
            return Ok(true);
        }

        let context = condition_context(run);
        let mut outgoing = runtime_edges_from_node(&run.definition_snapshot, &node_run.node_id)
            .into_iter()
            .filter(|edge| {
                edge.from_node_id == node_run.node_id
                    && edge_applies_to_node_status(edge.r#type, node_run.status)
            })
            .collect::<Vec<_>>();
        outgoing.sort_by_key(|edge| edge.priority);

        let mut matched_edges = Vec::new();
        let mut default_edge: Option<(WorkflowEdgeDto, JsonValue, JsonValue)> = None;
        for edge in outgoing {
            let evaluation = evaluate_workflow_condition(&edge.condition, &context);
            let condition_json = encode_workflow_condition(&edge.condition)?;
            project_store::insert_workflow_event(
                repo_root,
                project_id,
                &run.id,
                Some(&node_run.id),
                "workflow_edge_evaluated",
                &json!({
                    "edgeId": edge.id,
                    "fromNodeId": edge.from_node_id,
                    "toNodeId": edge.to_node_id,
                    "matched": evaluation.matched,
                    "condition": condition_json,
                    "evidence": evaluation.evidence.clone(),
                }),
            )?;
            if !evaluation.matched {
                continue;
            }
            if matches!(
                edge.condition,
                crate::commands::contracts::workflows::WorkflowConditionDto::Always
            ) {
                let should_replace = match default_edge.as_ref() {
                    Some((current, _, _)) => {
                        default_edge_specificity(edge.r#type)
                            >= default_edge_specificity(current.r#type)
                    }
                    None => true,
                };
                if should_replace {
                    default_edge = Some((edge, condition_json, evaluation.evidence));
                }
                continue;
            }
            matched_edges.push((edge, condition_json, evaluation.evidence));
            if routes_single_match(node) {
                break;
            }
        }
        if matched_edges.is_empty() {
            if let Some((edge, condition_json, evidence)) = default_edge {
                matched_edges.push((edge, condition_json, evidence));
            }
        }

        if !matched_edges.is_empty() {
            if node_run.status == WorkflowNodeRunStatusDto::Succeeded
                && had_prior_unsuccessful_attempt(run, node_run)
                && !has_metric_event_for_node(run, node_run, "recovery_success")
            {
                insert_workflow_metric_event(
                    repo_root,
                    project_id,
                    &run.id,
                    Some(&node_run.id),
                    "recovery_success",
                    &json!({
                        "nodeId": node_run.node_id,
                        "attemptNumber": node_run.attempt_number,
                    }),
                )?;
            }
            let mut route_decisions = Vec::with_capacity(matched_edges.len());
            for (edge, condition_json, evidence) in matched_edges {
                let resolution = loop_target_for_edge(repo_root, project_id, run, node_run, &edge)?;
                let target_node_id = resolution.target_node_id;
                let target_node = find_node(&run.definition_snapshot, &target_node_id).ok_or_else(
                    || {
                        CommandError::system_fault(
                            "workflow_target_node_missing",
                            format!(
                                "Workflow target node `{target_node_id}` was missing from its snapshot."
                            ),
                        )
                    },
                )?;
                let attempt = next_attempt_for_node(run, &target_node_id);
                route_decisions.push(project_store::WorkflowRouteDecisionRecord {
                    source_node_run_id: node_run.id.clone(),
                    source_status: resolution.source_status,
                    from_node_id: edge.from_node_id,
                    to_node_id: target_node_id.clone(),
                    edge_id: edge.id,
                    condition: condition_json,
                    evidence,
                    target_node_type: target_node.node_type().as_str().into(),
                    target_attempt_number: attempt,
                    target_idempotency_key: format!("{}:{target_node_id}:{attempt}", run.id),
                });
            }
            return project_store::commit_workflow_route(
                repo_root,
                project_id,
                &run.id,
                &route_decisions,
            );
        }

        project_store::insert_workflow_event(
            repo_root,
            project_id,
            &run.id,
            Some(&node_run.id),
            "workflow_route_missing",
            &json!({ "nodeId": node_run.node_id }),
        )?;
        project_store::update_workflow_run_status(
            repo_root,
            project_id,
            &run.id,
            WorkflowRunStatusDto::Paused,
            Some(WorkflowTerminalStatusDto::NeedsHuman),
            None,
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn start_eligible_nodes<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    let concurrency_limit = run.definition_snapshot.run_policy.concurrency_limit.max(1) as usize;
    let running_agent_count = run
        .nodes
        .iter()
        .filter(|node| {
            node.status == WorkflowNodeRunStatusDto::Running
                && matches!(
                    find_node(&run.definition_snapshot, &node.node_id),
                    Some(WorkflowNodeDto::Agent { .. })
                )
        })
        .count();
    let running_command_count = run
        .nodes
        .iter()
        .filter(|node| {
            matches!(
                node.status,
                WorkflowNodeRunStatusDto::Starting | WorkflowNodeRunStatusDto::Running
            ) && matches!(
                find_node(&run.definition_snapshot, &node.node_id),
                Some(WorkflowNodeDto::Command { .. })
            )
        })
        .count();
    for node_run in run
        .nodes
        .iter()
        .filter(|node| node.status == WorkflowNodeRunStatusDto::Eligible)
    {
        let Some(node) = find_node(&run.definition_snapshot, &node_run.node_id) else {
            continue;
        };
        match node {
            WorkflowNodeDto::Agent {
                title,
                agent_ref,
                input_bindings,
                run_overrides,
                ..
            } => {
                if running_agent_count >= concurrency_limit {
                    continue;
                }
                if let Some(conflict) = resource_conflict_for_node(run, node_run, node) {
                    if !has_node_event(run, node_run, "workflow_resource_conflict_wait") {
                        project_store::insert_workflow_event(
                            repo_root,
                            project_id,
                            &run.id,
                            Some(&node_run.id),
                            "workflow_resource_conflict_wait",
                            &json!({
                                "nodeId": node_run.node_id,
                                "blockedByNodeRunId": conflict.node_run_id,
                                "blockedByNodeId": conflict.node_id,
                                "scopes": conflict.scopes,
                            }),
                        )?;
                        return Ok(true);
                    }
                    continue;
                }
                if !project_store::claim_workflow_run_node_starting(
                    repo_root,
                    project_id,
                    &node_run.id,
                )? {
                    return Ok(true);
                }
                let input_bindings = runtime_input_bindings_for_node(
                    &run.definition_snapshot,
                    &node_run.node_id,
                    input_bindings,
                );
                start_agent_node(
                    app,
                    state,
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    title,
                    agent_ref,
                    &input_bindings,
                    run_overrides.as_ref(),
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Router { .. } => {
                project_store::update_workflow_run_node(
                    repo_root,
                    project_id,
                    &node_run.id,
                    WorkflowNodeRunStatusDto::Succeeded,
                    None,
                    None,
                    None,
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Merge {
                wait_policy,
                quorum,
                fail_fast,
                ..
            } => match evaluate_merge_node(run, node_run, *wait_policy, *quorum, *fail_fast) {
                MergeEvaluation::Waiting => {}
                MergeEvaluation::Succeeded => {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Succeeded,
                        None,
                        None,
                        None,
                    )?;
                    return Ok(true);
                }
                MergeEvaluation::Failed(failure_class) => {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Failed,
                        None,
                        None,
                        Some(failure_class),
                    )?;
                    return Ok(true);
                }
            },
            WorkflowNodeDto::Gate {
                required_checks,
                on_blocked,
                ..
            }
            | WorkflowNodeDto::StateCheckpoint {
                required_checks,
                on_blocked,
                ..
            } => {
                let context = condition_context(run);
                let passed = required_checks
                    .iter()
                    .all(|condition| evaluate_workflow_condition(condition, &context).matched);
                if passed {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Succeeded,
                        None,
                        None,
                        None,
                    )?;
                } else if on_blocked == "fail" {
                    project_store::update_workflow_run_node(
                        repo_root,
                        project_id,
                        &node_run.id,
                        WorkflowNodeRunStatusDto::Failed,
                        None,
                        None,
                        Some("gate_failed"),
                    )?;
                } else {
                    pause_at_checkpoint(repo_root, project_id, run, node_run, "gate_waiting")?;
                }
                return Ok(true);
            }
            WorkflowNodeDto::HumanCheckpoint { .. } => {
                pause_at_checkpoint(repo_root, project_id, run, node_run, "human_checkpoint")?;
                return Ok(true);
            }
            WorkflowNodeDto::StateRead {
                query,
                output_artifact_type,
                ..
            }
            | WorkflowNodeDto::StateQuery {
                query,
                output_artifact_type,
                ..
            } => {
                run_state_query_node(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    query,
                    output_artifact_type,
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::StateWrite { operation, .. }
            | WorkflowNodeDto::StatePatch { operation, .. } => {
                let operation = runtime_state_write_operation_for_node(
                    &run.definition_snapshot,
                    &node_run.node_id,
                    operation,
                );
                run_state_write_node(repo_root, project_id, run, node_run, &operation)?;
                return Ok(true);
            }
            WorkflowNodeDto::CollectionLoop {
                collection,
                item_artifact_type,
                sort_key,
                max_item_count,
                controls,
                ..
            } => {
                run_collection_loop_node(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    collection,
                    item_artifact_type,
                    sort_key.as_deref(),
                    *max_item_count,
                    controls,
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Command {
                command,
                args,
                allowed_commands,
                working_directory,
                timeout_seconds,
                success_exit_codes,
                output_contract,
                parser,
                ..
            } => {
                if running_command_count > 0 {
                    continue;
                }
                let args = runtime_template_strings_for_node(
                    &run.definition_snapshot,
                    &node_run.node_id,
                    args,
                );
                let working_directory = working_directory.as_ref().map(|value| {
                    runtime_template_string_for_node(
                        &run.definition_snapshot,
                        &node_run.node_id,
                        value,
                    )
                });
                spawn_command_node(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    command,
                    &args,
                    allowed_commands,
                    working_directory.as_deref(),
                    *timeout_seconds,
                    success_exit_codes,
                    output_contract,
                    parser.extraction,
                    parser.render_text_path.as_deref(),
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Subgraph {
                subgraph_id,
                input_bindings,
                output_contract,
                ..
            } => {
                run_subgraph_node(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    subgraph_id,
                    input_bindings,
                    output_contract,
                )?;
                return Ok(true);
            }
            WorkflowNodeDto::Terminal {
                terminal_status, ..
            } => {
                if let Some(context) =
                    subgraph_context_for_node_id(&run.definition_snapshot, &node_run.node_id)
                {
                    complete_subgraph_terminal(
                        repo_root,
                        project_id,
                        run,
                        node_run,
                        *terminal_status,
                        &context,
                    )?;
                } else {
                    complete_for_terminal(
                        state,
                        repo_root,
                        project_id,
                        run,
                        node_run,
                        *terminal_status,
                    )?;
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[allow(clippy::too_many_arguments)]
fn spawn_command_node(
    repo_root: &Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    command: &str,
    args: &[String],
    allowed_commands: &[String],
    working_directory: Option<&str>,
    timeout_seconds: u32,
    success_exit_codes: &[i32],
    output_contract: &WorkflowOutputContractDto,
    parser_extraction: WorkflowOutputExtractionDto,
    parser_render_text_path: Option<&str>,
) -> CommandResult<()> {
    if allowed_commands.is_empty() {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_command_allowlist_empty",
            "workflow_command_allowlist_empty",
            &format!(
                "Command node `{}` cannot run without an explicit command allowlist.",
                node_run.node_id
            ),
        );
    }
    if !workflow_command_is_allowlisted(command, allowed_commands) {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_command_not_allowed",
            "workflow_command_not_allowed",
            &format!(
                "Command node `{}` is not in its allowlist.",
                node_run.node_id
            ),
        );
    }
    if let Err(violation) = validate_workflow_command_policy(command, args) {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            violation.code,
            violation.code,
            &violation.message,
        );
    }
    let canonical_working_directory =
        match resolve_workflow_command_working_directory(repo_root, working_directory) {
            Ok(cwd) => cwd,
            Err(error) => {
                return fail_node_with_recoverable_error(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    "workflow_command_working_directory_invalid",
                    &error.code,
                    &error.message,
                );
            }
        };
    let mut registration = register_running_workflow_command(project_id, &run.id, &node_run.id)?;
    if !project_store::claim_workflow_command_node_starting(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        &registration.owner_instance_id,
        registration.owner_process_id,
        &registration.owner_process_birth_identity,
        &registration.lease_token,
        &crate::auth::now_timestamp(),
    )? {
        return Ok(());
    }
    if let Err(error) = registration.activate_persisted_lease(repo_root) {
        let _ = project_store::compare_and_set_workflow_run_node(
            repo_root,
            project_id,
            &node_run.id,
            &[WorkflowNodeRunStatusDto::Starting],
            WorkflowNodeRunStatusDto::Failed,
            None,
            None,
            Some("workflow_command_lease_heartbeat_failed"),
        );
        return Err(error);
    }
    if !project_store::compare_and_set_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        &[WorkflowNodeRunStatusDto::Starting],
        WorkflowNodeRunStatusDto::Running,
        None,
        None,
        None,
    )? {
        return Ok(());
    }

    let owned_repo_root = repo_root.to_path_buf();
    let owned_project_id = project_id.to_owned();
    let owned_run = run.clone();
    let owned_node_run = node_run.clone();
    let owned_command = command.to_owned();
    let owned_args = args.to_vec();
    let owned_allowed_commands = allowed_commands.to_vec();
    let owned_working_directory = canonical_working_directory.to_string_lossy().into_owned();
    let owned_success_exit_codes = success_exit_codes.to_vec();
    let owned_output_contract = output_contract.clone();
    let owned_parser_render_text_path = parser_render_text_path.map(ToOwned::to_owned);
    let spawn_result = thread::Builder::new()
        .name("workflow-command".into())
        .spawn(move || {
            let _ = run_command_node(
                &owned_repo_root,
                &owned_project_id,
                &owned_run,
                &owned_node_run,
                &owned_command,
                &owned_args,
                &owned_allowed_commands,
                Some(owned_working_directory.as_str()),
                timeout_seconds,
                &owned_success_exit_codes,
                &owned_output_contract,
                parser_extraction,
                owned_parser_render_text_path.as_deref(),
                registration,
            );
        });
    if let Err(error) = spawn_result {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_command_thread_spawn_failed",
            "workflow_command_thread_spawn_failed",
            &format!(
                "Xero could not start the execution thread for command node `{}`: {error}",
                node_run.node_id
            ),
        );
    }
    Ok(())
}

fn reconcile_interrupted_command_nodes(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
) -> CommandResult<bool> {
    for node_run in run.nodes.iter().filter(|node| {
        node.node_type == "command"
            && matches!(
                node.status,
                WorkflowNodeRunStatusDto::Starting | WorkflowNodeRunStatusDto::Running
            )
            && !running_workflow_command_is_registered(project_id, &run.id, &node.id)
    }) {
        let lease =
            project_store::load_workflow_command_lease(repo_root, project_id, &node_run.id)?;
        if lease.as_ref().is_some_and(workflow_command_lease_is_live) {
            continue;
        }
        let recovered = if let Some(lease) = lease.as_ref() {
            project_store::claim_interrupted_workflow_command(
                repo_root,
                project_id,
                &run.id,
                lease,
                COMMAND_INTERRUPTED_FAILURE_CLASS,
            )?
        } else {
            project_store::compare_and_set_workflow_run_node(
                repo_root,
                project_id,
                &node_run.id,
                &[
                    WorkflowNodeRunStatusDto::Starting,
                    WorkflowNodeRunStatusDto::Running,
                ],
                WorkflowNodeRunStatusDto::Stalled,
                None,
                None,
                Some(COMMAND_INTERRUPTED_FAILURE_CLASS),
            )?
        };
        if !recovered {
            return Ok(true);
        }
        if let Some((command_process_id, command_process_birth_identity)) =
            lease.as_ref().and_then(|lease| {
                lease
                    .command_process_id
                    .zip(lease.command_process_birth_identity.as_deref())
            })
        {
            terminate_orphaned_workflow_command(command_process_id, command_process_birth_identity);
        }
        project_store::insert_workflow_event(
            repo_root,
            project_id,
            &run.id,
            Some(&node_run.id),
            "workflow_command_interrupted",
            &json!({
                "nodeId": node_run.node_id,
                "failureClass": COMMAND_INTERRUPTED_FAILURE_CLASS,
                "previousOwnerInstanceId": lease.as_ref().map(|lease| lease.owner_instance_id.as_str()),
                "previousOwnerProcessId": lease.as_ref().map(|lease| lease.owner_process_id),
                "commandProcessId": lease.as_ref().and_then(|lease| lease.command_process_id),
                "commandProcessBirthIdentity": lease.as_ref().and_then(|lease| lease.command_process_birth_identity.as_deref()),
            }),
        )?;
        return Ok(true);
    }
    Ok(false)
}

fn workflow_command_lease_is_live(lease: &project_store::WorkflowCommandLeaseRecord) -> bool {
    // A live owner can still be executing an external side effect even when
    // its heartbeat thread is delayed. Without an execution epoch understood
    // by the child process, stealing on heartbeat age would let a replacement
    // route past the command while the old process continues to mutate state.
    process_identity_is_live(lease.owner_process_id, &lease.owner_process_birth_identity)
}

fn workflow_node_execution_is_live(
    _state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
) -> CommandResult<bool> {
    match find_node(&run.definition_snapshot, &node_run.node_id) {
        Some(WorkflowNodeDto::Agent { .. }) => {
            let snapshot = if let Some(runtime_run_id) = node_run.runtime_run_id.as_deref() {
                Some(project_store::load_agent_run(
                    repo_root,
                    project_id,
                    runtime_run_id,
                )?)
            } else {
                load_existing_agent_run_for_node(repo_root, project_id, node_run)?
            };
            Ok(snapshot.is_some_and(|snapshot| {
                matches!(
                    snapshot.run.status,
                    AgentRunStatus::Starting
                        | AgentRunStatus::Running
                        | AgentRunStatus::Paused
                        | AgentRunStatus::Cancelling
                )
            }))
        }
        Some(WorkflowNodeDto::Command { .. }) => {
            if running_workflow_command_is_registered(project_id, &run.id, &node_run.id) {
                return Ok(true);
            }
            Ok(
                project_store::load_workflow_command_lease(repo_root, project_id, &node_run.id)?
                    .as_ref()
                    .is_some_and(workflow_command_lease_is_live),
            )
        }
        _ => Ok(false),
    }
}

pub(super) fn terminate_workflow_node_execution(
    state: &DesktopState,
    repo_root: &Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
) -> CommandResult<()> {
    match find_node(&run.definition_snapshot, &node_run.node_id) {
        Some(WorkflowNodeDto::Agent { .. }) => {
            let snapshot = if let Some(runtime_run_id) = node_run.runtime_run_id.as_deref() {
                Some(project_store::load_agent_run(
                    repo_root,
                    project_id,
                    runtime_run_id,
                )?)
            } else {
                load_existing_agent_run_for_node(repo_root, project_id, node_run)?
            };
            let Some(snapshot) = snapshot else {
                return Ok(());
            };
            let snapshot = match resolve_agent_handoff_leaf(repo_root, project_id, snapshot, false)?
            {
                AgentHandoffLeafResolution::Leaf(snapshot) => snapshot,
                AgentHandoffLeafResolution::Incomplete(reason) => {
                    return Err(CommandError::retryable(
                        "workflow_agent_handoff_termination_unresolved",
                        reason,
                    ));
                }
            };
            let runtime_run_id = snapshot.run.run_id.clone();
            if matches!(
                snapshot.run.status,
                AgentRunStatus::Cancelled
                    | AgentRunStatus::HandedOff
                    | AgentRunStatus::Completed
                    | AgentRunStatus::Failed
            ) {
                return Ok(());
            }
            let runtime = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
            let cancelled = runtime.cancel_run(
                repo_root.to_path_buf(),
                project_id.to_owned(),
                runtime_run_id.clone(),
            )?;
            if matches!(
                cancelled.run.status,
                AgentRunStatus::Cancelled
                    | AgentRunStatus::HandedOff
                    | AgentRunStatus::Completed
                    | AgentRunStatus::Failed
            ) {
                Ok(())
            } else {
                Err(CommandError::retryable(
                    "workflow_agent_termination_incomplete",
                    format!(
                        "Owned-agent run `{runtime_run_id}` did not become terminal before Workflow control continued."
                    ),
                ))
            }
        }
        Some(WorkflowNodeDto::Command { .. }) => {
            if let Some(lease) =
                project_store::load_workflow_command_lease(repo_root, project_id, &node_run.id)?
            {
                if workflow_command_lease_is_live(&lease)
                    && lease.owner_instance_id != workflow_command_owner_instance_id()
                {
                    return Err(CommandError::retryable(
                        "workflow_command_active_in_another_process",
                        format!(
                            "Workflow command node `{}` is owned by another live Xero process and cannot be controlled here.",
                            node_run.id
                        ),
                    ));
                }
                if !workflow_command_lease_is_live(&lease) {
                    if let Some((process_id, birth_identity)) = lease
                        .command_process_id
                        .zip(lease.command_process_birth_identity.as_deref())
                    {
                        terminate_orphaned_workflow_command(process_id, birth_identity);
                    }
                    project_store::release_workflow_command_lease(
                        repo_root,
                        project_id,
                        &node_run.id,
                        &lease.owner_instance_id,
                        &lease.lease_token,
                    )?;
                }
            }
            terminate_running_workflow_commands(project_id, &run.id, Some(&node_run.id));
            let deadline = Instant::now() + COMMAND_TERMINATION_CONFIRM_TIMEOUT;
            loop {
                let registered =
                    running_workflow_command_is_registered(project_id, &run.id, &node_run.id);
                let lease = project_store::load_workflow_command_lease(
                    repo_root,
                    project_id,
                    &node_run.id,
                )?;
                if let Some(lease) = lease.as_ref() {
                    if workflow_command_lease_is_live(lease)
                        && lease.owner_instance_id != workflow_command_owner_instance_id()
                    {
                        return Err(CommandError::retryable(
                            "workflow_command_active_in_another_process",
                            format!(
                                "Workflow command node `{}` became owned by another live Xero process.",
                                node_run.id
                            ),
                        ));
                    }
                    if !workflow_command_lease_is_live(lease) {
                        if let Some((process_id, birth_identity)) = lease
                            .command_process_id
                            .zip(lease.command_process_birth_identity.as_deref())
                        {
                            terminate_orphaned_workflow_command(process_id, birth_identity);
                        }
                        project_store::release_workflow_command_lease(
                            repo_root,
                            project_id,
                            &node_run.id,
                            &lease.owner_instance_id,
                            &lease.lease_token,
                        )?;
                        continue;
                    }
                }
                if !registered && lease.is_none() {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(CommandError::retryable(
                        "workflow_command_termination_unconfirmed",
                        format!(
                            "Workflow command node `{}` did not confirm termination before the control timeout.",
                            node_run.id
                        ),
                    ));
                }
                thread::sleep(StdDuration::from_millis(25));
            }
        }
        _ => Ok(()),
    }
}

fn workflow_command_process_birth_identity(command_process_id: u32) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{command_process_id}/stat")).ok()?;
        let fields_after_name = stat.rsplit_once(") ")?.1;
        let start_ticks = fields_after_name.split_whitespace().nth(19)?;
        let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id").ok()?;
        return Some(format!("linux:{}:{start_ticks}", boot_id.trim()));
    }
    #[cfg(target_os = "macos")]
    {
        let process_id = libc::pid_t::try_from(command_process_id).ok()?;
        let mut info = std::mem::MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let size = std::mem::size_of::<libc::proc_bsdinfo>();
        let read = unsafe {
            libc::proc_pidinfo(
                process_id,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                i32::try_from(size).ok()?,
            )
        };
        if read != i32::try_from(size).ok()? {
            return None;
        }
        let info = unsafe { info.assume_init() };
        return Some(format!(
            "macos:{}:{}",
            info.pbi_start_tvsec, info.pbi_start_tvusec
        ));
    }
    #[cfg(windows)]
    {
        let script = format!(
            "(Get-Process -Id {command_process_id} -ErrorAction Stop).StartTime.ToUniversalTime().Ticks"
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let birth = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return (!birth.is_empty()).then(|| format!("windows:{birth}"));
    }
    #[cfg(all(unix, not(any(target_os = "linux", target_os = "macos"))))]
    {
        let output = Command::new("ps")
            .args(["-o", "lstart=", "-p", &command_process_id.to_string()])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let birth = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        return (!birth.is_empty()).then(|| format!("unix:{birth}"));
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = command_process_id;
        None
    }
}

fn terminate_orphaned_workflow_command(
    command_process_id: u32,
    expected_birth_identity: &str,
) -> bool {
    if workflow_command_process_birth_identity(command_process_id).as_deref()
        != Some(expected_birth_identity)
    {
        return false;
    }
    #[cfg(unix)]
    {
        let Ok(process_group_id) = libc::pid_t::try_from(command_process_id) else {
            return false;
        };
        let process_group_id = -process_group_id;
        return unsafe { libc::kill(process_group_id, libc::SIGKILL) } == 0;
    }
    #[cfg(windows)]
    {
        return Command::new("taskkill")
            .args(["/PID", &command_process_id.to_string(), "/T", "/F"])
            .status()
            .is_ok_and(|status| status.success());
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = command_process_id;
        let _ = expected_birth_identity;
        false
    }
}

struct ResourceConflict {
    node_run_id: String,
    node_id: String,
    scopes: Vec<String>,
}

fn resource_conflict_for_node(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    node: &WorkflowNodeDto,
) -> Option<ResourceConflict> {
    if run
        .definition_snapshot
        .run_policy
        .resource_conflict_policy
        .mode
        == WorkflowResourceConflictModeDto::AllowConflicts
    {
        return None;
    }
    let candidate_scopes = resource_scopes_for_node(&run.definition_snapshot, node);
    if candidate_scopes.is_empty() {
        return None;
    }

    for running in run.nodes.iter().filter(|running| {
        running.id != node_run.id
            && matches!(
                running.status,
                WorkflowNodeRunStatusDto::Starting | WorkflowNodeRunStatusDto::Running
            )
    }) {
        let Some(running_node) = find_node(&run.definition_snapshot, &running.node_id) else {
            continue;
        };
        let running_scopes = resource_scopes_for_node(&run.definition_snapshot, running_node);
        let overlap = overlapping_resource_scopes(&candidate_scopes, &running_scopes);
        if !overlap.is_empty() {
            return Some(ResourceConflict {
                node_run_id: running.id.clone(),
                node_id: running.node_id.clone(),
                scopes: overlap,
            });
        }
    }
    None
}

fn resource_scopes_for_node(
    definition: &WorkflowDefinitionDto,
    node: &WorkflowNodeDto,
) -> Vec<String> {
    match node {
        WorkflowNodeDto::Agent {
            resource_scopes, ..
        } if !resource_scopes.is_empty() => normalize_resource_scopes(resource_scopes),
        WorkflowNodeDto::Agent { .. } => normalize_resource_scopes(
            &definition
                .run_policy
                .resource_conflict_policy
                .default_scopes,
        ),
        _ => Vec::new(),
    }
}

fn normalize_resource_scopes(scopes: &[String]) -> Vec<String> {
    let mut normalized = scopes
        .iter()
        .map(|scope| scope.trim())
        .filter(|scope| !scope.is_empty())
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    normalized.sort();
    normalized.dedup();
    normalized
}

fn overlapping_resource_scopes(left: &[String], right: &[String]) -> Vec<String> {
    left.iter()
        .filter(|scope| right.iter().any(|candidate| candidate == *scope))
        .cloned()
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn start_agent_node<R: Runtime + 'static>(
    app: &AppHandle<R>,
    state: &DesktopState,
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    title: &str,
    agent_ref: &AgentRefDto,
    input_bindings: &[WorkflowInputBindingDto],
    run_overrides: Option<&WorkflowRunOverrideDto>,
) -> CommandResult<()> {
    let default_output_contract = WorkflowOutputContractDto::default();
    let output_contract = output_contract_for_node(&run.definition_snapshot, &node_run.node_id)
        .unwrap_or(&default_output_contract);
    let output_json_schema = artifact_schema_for_output(&run.definition_snapshot, output_contract);
    let prompt = match build_agent_node_prompt(
        &run.definition_snapshot.name,
        title,
        run_overrides.map(|overrides| overrides.prompt_preface.as_str()),
        output_contract,
        output_json_schema,
        run.initial_input.as_ref(),
        input_bindings,
        &run.artifacts,
    ) {
        Ok(prompt) => prompt,
        Err(error) if error.code == "workflow_required_input_missing" => {
            fail_node_with_recoverable_error(
                repo_root,
                project_id,
                run,
                node_run,
                "workflow_required_input_missing",
                &error.code,
                &error.message,
            )?;
            return Ok(());
        }
        Err(error) => return Err(error),
    };
    let controls = controls_for_agent_ref(
        repo_root,
        &run.definition_snapshot,
        agent_ref,
        run_overrides,
    )?;
    let expected_session_id = format!("workflow-agent-session:{}", node_run.idempotency_key);
    let session = project_store::create_agent_session_with_id(
        repo_root,
        &expected_session_id,
        &AgentSessionCreateRecord {
            project_id: project_id.into(),
            title: format!("Workflow: {title}"),
            summary: format!(
                "Node `{}` in Workflow `{}`.",
                node_run.node_id, run.definition_snapshot.name
            ),
            selected: false,
            session_kind: crate::db::project_store::AgentSessionKind::Standard,
        },
    )?;
    let resource_scopes = find_node(&run.definition_snapshot, &node_run.node_id)
        .map(|node| resource_scopes_for_node(&run.definition_snapshot, node))
        .unwrap_or_default();
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_agent_start_requested",
        &json!({
            "nodeId": node_run.node_id,
            "agentRef": agent_ref,
            "agentSessionId": session.agent_session_id.clone(),
            "resourceScopes": resource_scopes,
        }),
    )?;
    let agent_run = start_agent_task_blocking(
        app,
        state,
        StartAgentTaskRequestDto {
            project_id: project_id.into(),
            agent_session_id: expected_session_id,
            run_id: Some(node_run.idempotency_key.clone()),
            prompt,
            controls: Some(controls),
            attachments: Vec::new(),
        },
    )?;
    if !project_store::compare_and_set_workflow_run_node(
        repo_root,
        project_id,
        &node_run.id,
        &[WorkflowNodeRunStatusDto::Starting],
        WorkflowNodeRunStatusDto::Running,
        Some(&agent_run.run_id),
        Some(&session.agent_session_id),
        None,
    )? {
        let runtime = DesktopAgentCoreRuntime::new(state.agent_run_supervisor().clone());
        let _ = runtime.cancel_run(
            repo_root.to_path_buf(),
            project_id.to_owned(),
            agent_run.run_id,
        );
        return Ok(());
    }
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        &run.id,
        Some(&node_run.id),
        "workflow_agent_started",
        &json!({
            "runtimeRunId": agent_run.run_id,
            "agentSessionId": session.agent_session_id
        }),
    )?;
    Ok(())
}

fn load_existing_agent_run_for_node(
    repo_root: &std::path::Path,
    project_id: &str,
    node_run: &WorkflowRunNodeDto,
) -> CommandResult<Option<AgentRunSnapshotRecord>> {
    match project_store::load_agent_run(repo_root, project_id, &node_run.idempotency_key) {
        Ok(snapshot) => Ok(Some(snapshot)),
        Err(error) if error.code == "agent_run_not_found" => Ok(None),
        Err(error) => Err(error),
    }
}

fn run_state_query_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    query: &WorkflowStateQueryDto,
    output_artifact_type: &str,
) -> CommandResult<()> {
    project_store::complete_workflow_state_query_node_atomically(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        &node_run.node_id,
        query,
        output_artifact_type,
    )
    .map(|_| ())
}

fn run_state_write_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    operation: &WorkflowStateWriteOperationDto,
) -> CommandResult<()> {
    let resolved_operation = resolve_state_write_operation(operation, run, None)?;
    project_store::complete_workflow_state_write_node_atomically(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        &node_run.node_id,
        &resolved_operation,
    )
    .map(|_| ())
}

fn resolve_state_write_operation(
    operation: &WorkflowStateWriteOperationDto,
    run: &WorkflowRunDto,
    context: Option<&JsonValue>,
) -> CommandResult<WorkflowStateWriteOperationDto> {
    let resolved_idempotency_key = operation
        .idempotency_key
        .as_deref()
        .map(|value| {
            resolve_template_string_with_context(value, run, context)
                .map(|value| template_value_to_string(&value))
        })
        .transpose()?;
    let resolved_target_id = operation
        .target_id
        .as_deref()
        .map(|value| {
            resolve_template_string_with_context(value, run, context)
                .map(|value| template_value_to_string(&value))
        })
        .transpose()?;
    Ok(WorkflowStateWriteOperationDto {
        idempotency_key: resolved_idempotency_key,
        target_id: resolved_target_id,
        payload: resolve_template_object_with_context(
            &JsonValue::Object(operation.payload.clone()),
            run,
            context,
        )?
        .as_object()
        .cloned()
        .unwrap_or_default(),
        ..operation.clone()
    })
}

fn resolve_input_bindings_payload(
    run: &WorkflowRunDto,
    input_bindings: &[WorkflowInputBindingDto],
) -> CommandResult<JsonValue> {
    if input_bindings.is_empty() {
        return Ok(run.initial_input.clone().unwrap_or_else(|| json!({})));
    }
    let artifact_index = artifact_payloads_by_ref(run);
    let mut payload = serde_json::Map::new();
    for binding in input_bindings {
        let (name, required, value) = match binding {
            WorkflowInputBindingDto::RunInput {
                name,
                required,
                path,
                ..
            } => {
                let value =
                    lookup_run_input_binding(run.initial_input.as_ref(), name, path.as_deref())
                        .cloned();
                (name, *required, value)
            }
            WorkflowInputBindingDto::Artifact {
                name,
                required,
                artifact_ref,
                path,
                ..
            }
            | WorkflowInputBindingDto::State {
                name,
                required,
                state_ref: artifact_ref,
                path,
                ..
            } => {
                let value = artifact_index.get(artifact_ref).and_then(|artifact| {
                    path.as_deref()
                        .and_then(|path| json_path_lookup(artifact, path).cloned())
                        .or_else(|| Some((*artifact).clone()))
                });
                (name, *required, value)
            }
        };
        if let Some(value) = value {
            payload.insert(name.clone(), value);
        } else if required {
            return Err(CommandError::user_fixable(
                "workflow_required_input_missing",
                format!("Workflow subgraph input `{name}` is required but missing."),
            ));
        }
    }
    Ok(JsonValue::Object(payload))
}

fn runtime_input_bindings_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
    input_bindings: &[WorkflowInputBindingDto],
) -> Vec<WorkflowInputBindingDto> {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return input_bindings.to_vec();
    };
    let mut bindings = context
        .subgraph
        .input_bindings
        .iter()
        .map(|binding| subgraph_input_binding_for_parent(&context.parent_node_id, binding))
        .collect::<Vec<_>>();
    bindings.extend(input_bindings.iter().map(|binding| {
        namespace_input_binding(binding, &context.parent_node_id, context.subgraph)
    }));
    bindings
}

fn subgraph_input_binding_for_parent(
    parent_node_id: &str,
    binding: &WorkflowInputBindingDto,
) -> WorkflowInputBindingDto {
    let (name, required, prompt_label) = match binding {
        WorkflowInputBindingDto::RunInput {
            name,
            required,
            prompt_label,
            ..
        }
        | WorkflowInputBindingDto::Artifact {
            name,
            required,
            prompt_label,
            ..
        }
        | WorkflowInputBindingDto::State {
            name,
            required,
            prompt_label,
            ..
        } => (name.clone(), *required, prompt_label.clone()),
    };
    WorkflowInputBindingDto::Artifact {
        name: name.clone(),
        required,
        artifact_ref: format!("{parent_node_id}.{SUBGRAPH_INPUT_ARTIFACT_TYPE}"),
        path: Some(format!("$.{name}")),
        prompt_label,
    }
}

fn namespace_input_binding(
    binding: &WorkflowInputBindingDto,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> WorkflowInputBindingDto {
    match binding {
        WorkflowInputBindingDto::Artifact {
            name,
            required,
            artifact_ref,
            path,
            prompt_label,
        } => WorkflowInputBindingDto::Artifact {
            name: name.clone(),
            required: *required,
            artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            path: path.clone(),
            prompt_label: prompt_label.clone(),
        },
        WorkflowInputBindingDto::State {
            name,
            required,
            state_ref,
            path,
            prompt_label,
        } => WorkflowInputBindingDto::State {
            name: name.clone(),
            required: *required,
            state_ref: namespace_artifact_ref(parent_node_id, subgraph, state_ref),
            path: path.clone(),
            prompt_label: prompt_label.clone(),
        },
        WorkflowInputBindingDto::RunInput { .. } => binding.clone(),
    }
}

fn runtime_state_write_operation_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
    operation: &WorkflowStateWriteOperationDto,
) -> WorkflowStateWriteOperationDto {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return operation.clone();
    };
    let mut operation = operation.clone();
    operation.idempotency_key = operation
        .idempotency_key
        .as_deref()
        .map(|value| namespace_template_string(value, &context.parent_node_id, context.subgraph));
    operation.target_id = operation
        .target_id
        .as_deref()
        .map(|value| namespace_template_string(value, &context.parent_node_id, context.subgraph));
    operation.payload = operation
        .payload
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                namespace_template_value(value, &context.parent_node_id, context.subgraph),
            )
        })
        .collect();
    operation
}

fn runtime_template_strings_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
    values: &[String],
) -> Vec<String> {
    values
        .iter()
        .map(|value| runtime_template_string_for_node(definition, node_id, value))
        .collect()
}

fn runtime_template_string_for_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
    value: &str,
) -> String {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return value.to_string();
    };
    namespace_template_string(value, &context.parent_node_id, context.subgraph)
}

fn resolve_template_object_with_context(
    value: &JsonValue,
    run: &WorkflowRunDto,
    context: Option<&JsonValue>,
) -> CommandResult<JsonValue> {
    match value {
        JsonValue::Object(map) => {
            let mut resolved = serde_json::Map::new();
            for (key, value) in map {
                resolved.insert(
                    key.clone(),
                    resolve_template_object_with_context(value, run, context)?,
                );
            }
            Ok(JsonValue::Object(resolved))
        }
        JsonValue::Array(values) => values
            .iter()
            .map(|value| resolve_template_object_with_context(value, run, context))
            .collect::<Result<Vec<_>, _>>()
            .map(JsonValue::Array),
        JsonValue::String(text) => resolve_template_string_with_context(text, run, context),
        value => Ok(value.clone()),
    }
}

fn resolve_template_string(text: &str, run: &WorkflowRunDto) -> CommandResult<JsonValue> {
    resolve_template_string_with_context(text, run, None)
}

fn resolve_template_string_with_context(
    text: &str,
    run: &WorkflowRunDto,
    context: Option<&JsonValue>,
) -> CommandResult<JsonValue> {
    let trimmed = text.trim();
    if let Some(expression) = trimmed
        .strip_prefix("{{")
        .and_then(|value| value.strip_suffix("}}"))
    {
        return resolve_template_expression_with_context(expression.trim(), run, context);
    }

    let mut rendered = String::new();
    let mut remainder = text;
    loop {
        let Some(start) = remainder.find("{{") else {
            rendered.push_str(remainder);
            break;
        };
        rendered.push_str(&remainder[..start]);
        let after_start = &remainder[start + 2..];
        let Some(end) = after_start.find("}}") else {
            rendered.push_str(&remainder[start..]);
            break;
        };
        let expression = after_start[..end].trim();
        let value = resolve_template_expression_with_context(expression, run, context)?;
        rendered.push_str(&template_value_to_string(&value));
        remainder = &after_start[end + 2..];
    }
    Ok(JsonValue::String(rendered))
}

fn resolve_template_expression_with_context(
    expression: &str,
    run: &WorkflowRunDto,
    context: Option<&JsonValue>,
) -> CommandResult<JsonValue> {
    if expression == "decision" {
        return context.cloned().ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_template_context_missing",
                "Workflow template references `decision`, but no checkpoint decision context is available.",
            )
        });
    }
    if let Some(path) = expression.strip_prefix("decision.") {
        return lookup_template_context(context, &format!("$.{path}"));
    }
    if let Some(path) = expression.strip_prefix("decision:") {
        return lookup_template_context(context, path.trim());
    }
    if expression == "input" {
        return run.initial_input.clone().ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_template_input_missing",
                "Workflow state write template references `input`, but the run has no initial input.",
            )
        });
    }
    if let Some(path) = expression.strip_prefix("input.") {
        return lookup_initial_input(run, &format!("$.{path}"));
    }
    if let Some(path) = expression.strip_prefix("input:") {
        return lookup_initial_input(run, path.trim());
    }
    if expression == "run.id" {
        return Ok(JsonValue::String(run.id.clone()));
    }
    if expression == "workflow.id" {
        return Ok(JsonValue::String(run.workflow_id.clone()));
    }
    if let Some(rest) = expression
        .strip_prefix("artifact.")
        .or_else(|| expression.strip_prefix("state."))
    {
        return lookup_artifact_template_ref(run, rest);
    }
    if let Some(rest) = expression
        .strip_prefix("artifact:")
        .or_else(|| expression.strip_prefix("state:"))
    {
        return lookup_artifact_template_ref(run, rest.trim());
    }

    Err(CommandError::user_fixable(
        "workflow_template_expression_unknown",
        format!("Workflow template expression `{expression}` is not supported."),
    ))
}

fn lookup_template_context(context: Option<&JsonValue>, path: &str) -> CommandResult<JsonValue> {
    let context = context.ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_context_missing",
            format!(
                "Workflow template references `{path}`, but no checkpoint decision context is available."
            ),
        )
    })?;
    json_path_lookup(context, path).cloned().ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_context_missing",
            format!("Workflow template could not resolve checkpoint decision path `{path}`."),
        )
    })
}

fn lookup_initial_input(run: &WorkflowRunDto, path: &str) -> CommandResult<JsonValue> {
    let input = run.initial_input.as_ref().ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_input_missing",
            format!("Workflow template references `{path}`, but the run has no initial input."),
        )
    })?;
    json_path_lookup(input, path).cloned().ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_input_missing",
            format!("Workflow template could not resolve initial input path `{path}`."),
        )
    })
}

fn lookup_artifact_template_ref(
    run: &WorkflowRunDto,
    expression: &str,
) -> CommandResult<JsonValue> {
    let (artifact_ref, path) = split_artifact_template_ref(expression);
    let index = artifact_payloads_by_ref(run);
    let payload = index.get(&artifact_ref).ok_or_else(|| {
        CommandError::user_fixable(
            "workflow_template_artifact_missing",
            format!(
                "Workflow template references `{artifact_ref}`, but no matching artifact exists."
            ),
        )
    })?;
    if let Some(path) = path {
        return json_path_lookup(payload, &path).cloned().ok_or_else(|| {
            CommandError::user_fixable(
                "workflow_template_artifact_path_missing",
                format!("Workflow template could not resolve `{artifact_ref}{path}`."),
            )
        });
    }
    Ok((*payload).to_owned())
}

fn split_artifact_template_ref(expression: &str) -> (String, Option<String>) {
    if let Some((artifact_ref, path)) = expression.split_once(' ') {
        let trimmed_path = path.trim();
        return (
            artifact_ref.trim().to_string(),
            (!trimmed_path.is_empty()).then(|| trimmed_path.to_string()),
        );
    }

    let parts = expression.split('.').collect::<Vec<_>>();
    if parts.len() >= 3 {
        (
            format!("{}.{}", parts[0], parts[1]),
            Some(format!("$.{}", parts[2..].join("."))),
        )
    } else {
        (expression.trim().to_string(), None)
    }
}

fn namespace_template_value(
    value: &JsonValue,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> JsonValue {
    match value {
        JsonValue::String(text) => {
            JsonValue::String(namespace_template_string(text, parent_node_id, subgraph))
        }
        JsonValue::Array(values) => JsonValue::Array(
            values
                .iter()
                .map(|value| namespace_template_value(value, parent_node_id, subgraph))
                .collect(),
        ),
        JsonValue::Object(map) => JsonValue::Object(
            map.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        namespace_template_value(value, parent_node_id, subgraph),
                    )
                })
                .collect(),
        ),
        value => value.clone(),
    }
}

fn namespace_template_string(
    text: &str,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> String {
    let mut rendered = String::new();
    let mut remainder = text;
    loop {
        let Some(start) = remainder.find("{{") else {
            rendered.push_str(remainder);
            break;
        };
        rendered.push_str(&remainder[..start]);
        let after_start = &remainder[start + 2..];
        let Some(end) = after_start.find("}}") else {
            rendered.push_str(&remainder[start..]);
            break;
        };
        let expression = after_start[..end].trim();
        rendered.push_str("{{");
        rendered.push_str(&namespace_template_expression(
            expression,
            parent_node_id,
            subgraph,
        ));
        rendered.push_str("}}");
        remainder = &after_start[end + 2..];
    }
    rendered
}

fn namespace_template_expression(
    expression: &str,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> String {
    for prefix in ["artifact:", "state:"] {
        if let Some(rest) = expression.strip_prefix(prefix) {
            let (artifact_ref, path) = split_artifact_template_ref(rest.trim());
            let namespaced_ref = namespace_artifact_ref(parent_node_id, subgraph, &artifact_ref);
            return match path {
                Some(path) => format!("{prefix}{namespaced_ref} {path}"),
                None => format!("{prefix}{namespaced_ref}"),
            };
        }
    }
    for prefix in ["artifact.", "state."] {
        if let Some(rest) = expression.strip_prefix(prefix) {
            let (artifact_ref, path) = split_artifact_template_ref(rest.trim());
            let namespaced_ref = namespace_artifact_ref(parent_node_id, subgraph, &artifact_ref);
            let colon_prefix = if prefix == "artifact." {
                "artifact:"
            } else {
                "state:"
            };
            return match path {
                Some(path) => format!("{colon_prefix}{namespaced_ref} {path}"),
                None => format!("{colon_prefix}{namespaced_ref}"),
            };
        }
    }
    expression.to_string()
}

fn namespace_artifact_ref(
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
    artifact_ref: &str,
) -> String {
    let Some((node_ref, artifact_type)) = artifact_ref.split_once('.') else {
        return artifact_ref.to_string();
    };
    if !subgraph.nodes.iter().any(|node| node.id() == node_ref) {
        return artifact_ref.to_string();
    }
    format!(
        "{}.{}",
        namespaced_subgraph_node_id(parent_node_id, node_ref),
        artifact_type
    )
}

fn namespace_node_ref(
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
    node_id: &str,
) -> String {
    if subgraph.nodes.iter().any(|node| node.id() == node_id) {
        namespaced_subgraph_node_id(parent_node_id, node_id)
    } else {
        node_id.to_string()
    }
}

fn namespace_loop_key(parent_node_id: &str, loop_key: &str) -> String {
    if loop_key.contains(SUBGRAPH_NODE_SEPARATOR) {
        loop_key.to_string()
    } else {
        format!("{parent_node_id}{SUBGRAPH_NODE_SEPARATOR}{loop_key}")
    }
}

fn artifact_payloads_by_ref(run: &WorkflowRunDto) -> BTreeMap<String, &JsonValue> {
    let node_id_by_run_id = run
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node.node_id.as_str()))
        .collect::<BTreeMap<_, _>>();
    let mut index = BTreeMap::new();
    for artifact in &run.artifacts {
        if let Some(node_id) = node_id_by_run_id.get(artifact.producer_node_run_id.as_str()) {
            index.insert(
                format!("{node_id}.{}", artifact.artifact_type),
                &artifact.payload,
            );
        }
    }
    index
}

fn template_value_to_string(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        JsonValue::Null => String::new(),
        value => serde_json::to_string(value).unwrap_or_else(|_| value.to_string()),
    }
}

#[allow(clippy::too_many_arguments)]
fn run_collection_loop_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    collection: &WorkflowStateQueryDto,
    item_artifact_type: &str,
    sort_key: Option<&str>,
    max_item_count: u32,
    controls: &WorkflowCollectionLoopControlsDto,
) -> CommandResult<()> {
    let mut payload = project_store::query_delivery_state(repo_root, project_id, collection)?;
    let mut records = payload
        .get_mut("records")
        .and_then(JsonValue::as_array_mut)
        .map(std::mem::take)
        .unwrap_or_default();
    if let Some(sort_key) = sort_key {
        records.sort_by(|left, right| {
            compare_json_values_for_runtime(
                json_path_lookup(left, sort_key),
                json_path_lookup(right, sort_key),
            )
        });
    }
    let control_selection = collection_control_selection(run.initial_input.as_ref(), controls);
    records = apply_collection_controls(records, run.initial_input.as_ref(), controls);

    let processed = processed_collection_item_ids(run, &node_run.node_id);
    let processed_count = processed.len() as u32;
    let next = if processed_count >= max_item_count {
        None
    } else {
        records
            .iter()
            .find(|record| {
                record
                    .get("id")
                    .and_then(JsonValue::as_str)
                    .map(|id| !processed.contains(id))
                    .unwrap_or(false)
            })
            .cloned()
    };
    let has_item = next.is_some();
    let item_id = next
        .as_ref()
        .and_then(|record| record.get("id"))
        .and_then(JsonValue::as_str)
        .map(ToOwned::to_owned);
    let artifact_payload = json!({
        "hasItem": has_item,
        "item": next,
        "itemId": item_id,
        "processedCount": processed_count,
        "remainingCount": records.len().saturating_sub(processed.len()),
        "maxItemCount": max_item_count,
        "partialSelection": control_selection.has_selection,
        "controls": {
            "only": control_selection.only_values,
            "from": control_selection.from_value,
            "to": control_selection.to_value,
        },
    });
    project_store::complete_prepared_workflow_state_node_atomically(
        repo_root,
        project_id,
        &project_store::WorkflowPreparedStateNodeCompletionRecord {
            run_id: run.id.clone(),
            node_run_id: node_run.id.clone(),
            artifact_type: item_artifact_type.to_owned(),
            payload: artifact_payload,
            render_text: item_id.clone().or_else(|| {
                Some(
                    if has_item {
                        "Collection item selected"
                    } else {
                        "Collection complete"
                    }
                    .to_owned(),
                )
            }),
            event_type: if has_item {
                "workflow_collection_item_started"
            } else {
                "workflow_collection_completed"
            }
            .to_owned(),
            event: json!({
                "loopNodeId": node_run.node_id,
                "itemId": item_id,
                "processedCount": processed_count,
                "remainingCount": records.len().saturating_sub(processed.len()),
            }),
        },
    )
    .map(|_| ())
}

fn apply_collection_controls(
    records: Vec<JsonValue>,
    initial_input: Option<&JsonValue>,
    controls: &WorkflowCollectionLoopControlsDto,
) -> Vec<JsonValue> {
    let only_values = controls
        .only_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .map(control_values);
    let from_value = controls
        .from_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .and_then(control_scalar);
    let to_value = controls
        .to_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .and_then(control_scalar);

    records
        .into_iter()
        .filter(|record| {
            let key = collection_record_key(record);
            if let Some(only_values) = &only_values {
                let Some(key) = key.as_ref() else {
                    return false;
                };
                if !only_values
                    .iter()
                    .any(|value| control_values_equal(value, key))
                {
                    return false;
                }
            }
            if let (Some(from), Some(key)) = (from_value.as_ref(), key.as_ref()) {
                if compare_control_values(key, from) == std::cmp::Ordering::Less {
                    return false;
                }
            }
            if let (Some(to), Some(key)) = (to_value.as_ref(), key.as_ref()) {
                if compare_control_values(key, to) == std::cmp::Ordering::Greater {
                    return false;
                }
            }
            true
        })
        .collect()
}

#[derive(Debug, Clone)]
struct CollectionControlSelection {
    only_values: Option<Vec<JsonValue>>,
    from_value: Option<JsonValue>,
    to_value: Option<JsonValue>,
    has_selection: bool,
}

fn collection_control_selection(
    initial_input: Option<&JsonValue>,
    controls: &WorkflowCollectionLoopControlsDto,
) -> CollectionControlSelection {
    let only_values = controls
        .only_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .map(control_values)
        .filter(|values| !values.is_empty());
    let from_value = controls
        .from_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .and_then(control_scalar);
    let to_value = controls
        .to_input_path
        .as_deref()
        .and_then(|path| initial_input.and_then(|input| json_path_lookup(input, path)))
        .and_then(control_scalar);
    let has_selection = only_values.is_some() || from_value.is_some() || to_value.is_some();
    CollectionControlSelection {
        only_values,
        from_value,
        to_value,
        has_selection,
    }
}

fn processed_collection_item_ids(run: &WorkflowRunDto, loop_node_id: &str) -> BTreeSet<String> {
    let loop_node_run_ids = run
        .nodes
        .iter()
        .filter(|node| {
            node.node_id == loop_node_id && node.status == WorkflowNodeRunStatusDto::Succeeded
        })
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>();
    run.artifacts
        .iter()
        .filter(|artifact| loop_node_run_ids.contains(artifact.producer_node_run_id.as_str()))
        .filter_map(|artifact| {
            artifact
                .payload
                .get("itemId")
                .and_then(JsonValue::as_str)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
        })
        .collect()
}

fn collection_record_key(record: &JsonValue) -> Option<JsonValue> {
    record
        .get("phaseKey")
        .or_else(|| record.get("phase_key"))
        .or_else(|| record.get("id"))
        .or_else(|| record.get("sortOrder"))
        .or_else(|| record.get("sort_order"))
        .cloned()
}

fn control_values(value: &JsonValue) -> Vec<JsonValue> {
    match value {
        JsonValue::Array(values) => values.clone(),
        JsonValue::String(text) => text
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| JsonValue::String(value.to_string()))
            .collect(),
        value => vec![value.clone()],
    }
}

fn control_scalar(value: &JsonValue) -> Option<JsonValue> {
    match value {
        JsonValue::Array(values) => values.first().cloned(),
        JsonValue::Null => None,
        value => Some(value.clone()),
    }
}

fn control_values_equal(left: &JsonValue, right: &JsonValue) -> bool {
    if left == right {
        return true;
    }
    left.as_str().map(str::trim) == right.as_str().map(str::trim)
        || left
            .as_f64()
            .zip(right.as_f64())
            .is_some_and(|(left, right)| (left - right).abs() < f64::EPSILON)
}

fn compare_control_values(left: &JsonValue, right: &JsonValue) -> std::cmp::Ordering {
    match (left.as_f64(), right.as_f64()) {
        (Some(left), Some(right)) => left
            .partial_cmp(&right)
            .unwrap_or(std::cmp::Ordering::Equal),
        _ => template_value_to_string(left).cmp(&template_value_to_string(right)),
    }
}

fn resolve_workflow_command_working_directory(
    repo_root: &Path,
    working_directory: Option<&str>,
) -> CommandResult<PathBuf> {
    let canonical_repo_root = repo_root.canonicalize().map_err(|error| {
        CommandError::system_fault(
            "workflow_project_root_unavailable",
            format!(
                "Xero could not resolve the Workflow project root `{}`: {error}",
                repo_root.display()
            ),
        )
    })?;
    let requested = working_directory
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| canonical_repo_root.clone());
    let candidate = if requested.is_absolute() {
        requested
    } else {
        canonical_repo_root.join(requested)
    };
    let canonical_candidate = candidate.canonicalize().map_err(|error| {
        CommandError::user_fixable(
            "workflow_command_working_directory_invalid",
            format!(
                "Workflow command working directory `{}` does not resolve to an existing directory: {error}",
                candidate.display()
            ),
        )
    })?;
    if !canonical_candidate.is_dir() {
        return Err(CommandError::user_fixable(
            "workflow_command_working_directory_invalid",
            format!(
                "Workflow command working directory `{}` is not a directory.",
                canonical_candidate.display()
            ),
        ));
    }
    if !canonical_candidate.starts_with(&canonical_repo_root) {
        return Err(CommandError::user_fixable(
            "workflow_command_working_directory_outside_project",
            format!(
                "Workflow command working directory `{}` resolves outside project root `{}`.",
                canonical_candidate.display(),
                canonical_repo_root.display()
            ),
        ));
    }
    Ok(canonical_candidate)
}

struct OpenedWorkflowCommandWorkingDirectory {
    display_path: PathBuf,
    #[cfg(unix)]
    directory: File,
}

impl OpenedWorkflowCommandWorkingDirectory {
    fn display_path(&self) -> &Path {
        &self.display_path
    }

    fn configure_process(&self, command: &mut Command) -> io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::{fd::AsRawFd, unix::process::CommandExt};

            let directory = self.directory.try_clone()?;
            unsafe {
                command.pre_exec(move || {
                    if libc::fchdir(directory.as_raw_fd()) == 0 {
                        Ok(())
                    } else {
                        Err(io::Error::last_os_error())
                    }
                });
            }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            let _ = command;
            Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "secure directory-handle command launch is unavailable on this platform",
            ))
        }
    }

    #[cfg(unix)]
    fn repository_access(&self) -> io::Result<(File, PathBuf)> {
        use std::os::fd::AsRawFd;

        let directory = self.directory.try_clone()?;
        let descriptor = directory.as_raw_fd();
        #[cfg(target_os = "macos")]
        let path = {
            use std::{ffi::CStr, os::unix::ffi::OsStrExt};

            let mut buffer = [0_i8; libc::PATH_MAX as usize];
            let result = unsafe { libc::fcntl(descriptor, libc::F_GETPATH, buffer.as_mut_ptr()) };
            if result < 0 {
                return Err(io::Error::last_os_error());
            }
            let bytes = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_bytes();
            PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
        };
        #[cfg(target_os = "linux")]
        let path = std::fs::read_link(format!("/proc/self/fd/{descriptor}"))?;
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        let path = std::fs::canonicalize(format!("/dev/fd/{descriptor}"))?;
        ensure_path_matches_open_directory(&path, &directory)?;
        Ok((directory, path))
    }
}

#[cfg(unix)]
fn ensure_path_matches_open_directory(path: &Path, directory: &File) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;

    let path_metadata = std::fs::metadata(path)?;
    let descriptor_metadata = directory.metadata()?;
    if path_metadata.dev() == descriptor_metadata.dev()
        && path_metadata.ino() == descriptor_metadata.ino()
    {
        Ok(())
    } else {
        Err(io::Error::other(
            "the pinned Workflow command directory no longer matches its filesystem path",
        ))
    }
}

fn open_workflow_command_working_directory(
    repo_root: &Path,
    working_directory: Option<&str>,
) -> CommandResult<OpenedWorkflowCommandWorkingDirectory> {
    #[cfg(not(unix))]
    {
        let _ = (repo_root, working_directory);
        return Err(CommandError::user_fixable(
            "workflow_command_secure_launch_unsupported",
            "Secure Workflow command execution is not available on this platform.",
        ));
    }

    #[cfg(unix)]
    {
        use std::{
            ffi::CString,
            os::{
                fd::FromRawFd,
                unix::{ffi::OsStrExt, fs::OpenOptionsExt},
            },
            path::Component,
        };

        let canonical_repo_root = repo_root.canonicalize().map_err(|error| {
            CommandError::system_fault(
                "workflow_project_root_unavailable",
                format!(
                    "Xero could not resolve the Workflow project root `{}`: {error}",
                    repo_root.display()
                ),
            )
        })?;
        let requested = working_directory
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        let relative = match requested.as_ref() {
            None => PathBuf::new(),
            Some(requested) if requested.is_absolute() => requested
                .strip_prefix(&canonical_repo_root)
                .map(Path::to_path_buf)
                .map_err(|_| {
                    CommandError::user_fixable(
                        "workflow_command_working_directory_outside_project",
                        format!(
                            "Workflow command working directory `{}` is outside project root `{}`.",
                            requested.display(),
                            canonical_repo_root.display()
                        ),
                    )
                })?,
            Some(requested) => requested.clone(),
        };

        let mut segments = Vec::new();
        for component in relative.components() {
            match component {
                Component::CurDir => {}
                Component::Normal(segment) => segments.push(segment.to_owned()),
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(CommandError::user_fixable(
                        "workflow_command_working_directory_outside_project",
                        format!(
                            "Workflow command working directory `{}` cannot traverse outside the project.",
                            relative.display()
                        ),
                    ));
                }
            }
        }

        let mut directory = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
            .open(&canonical_repo_root)
            .map_err(|error| {
                CommandError::system_fault(
                    "workflow_project_root_unavailable",
                    format!(
                        "Xero could not securely open Workflow project root `{}`: {error}",
                        canonical_repo_root.display()
                    ),
                )
            })?;
        let mut display_path = canonical_repo_root;
        for segment in segments {
            let name = CString::new(segment.as_bytes()).map_err(|_| {
                CommandError::user_fixable(
                    "workflow_command_working_directory_invalid",
                    "Workflow command working directories cannot contain NUL bytes.",
                )
            })?;
            use std::os::fd::AsRawFd;
            let descriptor = unsafe {
                libc::openat(
                    directory.as_raw_fd(),
                    name.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                )
            };
            if descriptor < 0 {
                let error = io::Error::last_os_error();
                return Err(CommandError::user_fixable(
                    "workflow_command_working_directory_invalid",
                    format!(
                        "Workflow command working directory `{}` could not be securely opened without following links: {error}",
                        display_path.join(&segment).display()
                    ),
                ));
            }
            directory = unsafe { File::from_raw_fd(descriptor) };
            display_path.push(segment);
        }

        Ok(OpenedWorkflowCommandWorkingDirectory {
            display_path,
            directory,
        })
    }
}

fn workflow_command_is_allowlisted(command: &str, allowed_commands: &[String]) -> bool {
    !allowed_commands.is_empty() && allowed_commands.iter().any(|allowed| allowed == command)
}

#[allow(clippy::too_many_arguments)]
fn run_command_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    command: &str,
    args: &[String],
    allowed_commands: &[String],
    working_directory: Option<&str>,
    timeout_seconds: u32,
    success_exit_codes: &[i32],
    output_contract: &WorkflowOutputContractDto,
    parser_extraction: WorkflowOutputExtractionDto,
    parser_render_text_path: Option<&str>,
    registration: RunningCommandRegistration,
) -> CommandResult<()> {
    let control = registration.control.clone();
    if allowed_commands.is_empty() {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_command_allowlist_empty",
            "workflow_command_allowlist_empty",
            &format!(
                "Command node `{}` cannot run without an explicit command allowlist.",
                node_run.node_id
            ),
        );
    }
    if !workflow_command_is_allowlisted(command, allowed_commands) {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_command_not_allowed",
            "workflow_command_not_allowed",
            &format!(
                "Command node `{}` is not in its allowlist.",
                node_run.node_id
            ),
        );
    }
    let resolved_args = args
        .iter()
        .map(|arg| resolve_template_string(arg, run).map(|value| template_value_to_string(&value)))
        .collect::<CommandResult<Vec<_>>>()?;
    if let Err(violation) = validate_workflow_command_policy(command, &resolved_args) {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            violation.code,
            violation.code,
            &violation.message,
        );
    }
    let cwd = match open_workflow_command_working_directory(repo_root, working_directory) {
        Ok(cwd) => cwd,
        Err(error) => {
            return fail_node_with_recoverable_error(
                repo_root,
                project_id,
                run,
                node_run,
                "workflow_command_working_directory_invalid",
                &error.code,
                &error.message,
            );
        }
    };
    if control.termination_requested() {
        return Ok(());
    }
    if command == "git" {
        return run_internal_git_status_node(
            repo_root,
            project_id,
            run,
            node_run,
            command,
            &resolved_args,
            timeout_seconds,
            success_exit_codes,
            output_contract,
            parser_extraction,
            parser_render_text_path,
            registration,
            &cwd,
        );
    }
    let executable = match resolve_workflow_command_executable(command) {
        Ok(executable) => executable,
        Err(violation) => {
            return fail_node_with_recoverable_error(
                repo_root,
                project_id,
                run,
                node_run,
                violation.code,
                violation.code,
                &violation.message,
            );
        }
    };
    let started = Instant::now();
    let mut process = Command::new(executable);
    harden_workflow_command_process(command, repo_root, &mut process);
    append_workflow_command_arguments(command, &resolved_args, &mut process);
    process.stdout(Stdio::piped()).stderr(Stdio::piped());
    if let Err(error) = cwd.configure_process(&mut process) {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_command_working_directory_invalid",
            "workflow_command_working_directory_invalid",
            &format!(
                "Xero could not pin the working directory for command node `{}`: {error}",
                node_run.node_id
            ),
        );
    }
    configure_process_tree_root(&mut process);
    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            return fail_node_with_recoverable_error(
                repo_root,
                project_id,
                run,
                node_run,
                "workflow_command_spawn_failed",
                "workflow_command_spawn_failed",
                &format!(
                    "Xero could not start command node `{}`: {error}",
                    node_run.node_id
                ),
            );
        }
    };
    if let Err(error) = register_process_tree_root(&child) {
        let _ = terminate_process_tree(&mut child);
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_command_tree_registration_failed",
            "workflow_command_tree_registration_failed",
            &format!(
                "Xero could not establish process-tree ownership for command node `{}`: {error}",
                node_run.node_id
            ),
        );
    }
    let command_process_id = child.id();
    let command_process_birth_identity =
        workflow_command_process_birth_identity(command_process_id);
    match project_store::attach_workflow_command_process(
        repo_root,
        project_id,
        &node_run.id,
        &registration.owner_instance_id,
        &registration.lease_token,
        command_process_id,
        command_process_birth_identity.as_deref(),
        &crate::auth::now_timestamp(),
    ) {
        Ok(true) => {}
        Ok(false) => {
            let _ = terminate_process_tree(&mut child);
            return Ok(());
        }
        Err(error) => {
            let _ = terminate_process_tree(&mut child);
            return Err(error);
        }
    }
    let Some(stdout) = child.stdout.take() else {
        let _ = terminate_process_tree(&mut child);
        return Err(CommandError::system_fault(
            "workflow_command_stdout_unavailable",
            format!(
                "Workflow command node `{}` started without a stdout pipe.",
                node_run.node_id
            ),
        ));
    };
    let Some(stderr) = child.stderr.take() else {
        let _ = terminate_process_tree(&mut child);
        return Err(CommandError::system_fault(
            "workflow_command_stderr_unavailable",
            format!(
                "Workflow command node `{}` started without a stderr pipe.",
                node_run.node_id
            ),
        ));
    };
    let stdout_reader = spawn_bounded_command_reader(stdout);
    let stderr_reader = spawn_bounded_command_reader(stderr);
    control.attach_child(child);
    let timeout = StdDuration::from_secs(timeout_seconds.into());
    let (exit_status, timed_out, externally_terminated) = loop {
        if control.termination_requested() {
            let status = control.terminate().map_err(|error| {
                CommandError::retryable(
                    "workflow_command_termination_failed",
                    format!(
                        "Xero could not terminate command node `{}`: {error}",
                        node_run.node_id
                    ),
                )
            })?;
            break (status, false, true);
        }
        match control.try_wait() {
            Ok(Some(status)) => break (status, false, false),
            Ok(None) => {}
            Err(error) => {
                let _ = control.terminate();
                return Err(CommandError::retryable(
                    "workflow_command_wait_failed",
                    format!(
                        "Xero could not poll command node `{}`: {error}",
                        node_run.node_id
                    ),
                ));
            }
        }
        if started.elapsed() >= timeout {
            let status = control.terminate().map_err(|error| {
                CommandError::retryable(
                    "workflow_command_termination_failed",
                    format!(
                        "Xero could not terminate timed-out command node `{}`: {error}",
                        node_run.node_id
                    ),
                )
            })?;
            break (status, true, false);
        }
        std::thread::sleep(StdDuration::from_millis(25));
    };
    let stream_drain_deadline = Instant::now() + COMMAND_STREAM_DRAIN_TIMEOUT;
    let stdout_capture = finish_bounded_command_reader(stdout_reader, stream_drain_deadline);
    let stderr_capture = finish_bounded_command_reader(stderr_reader, stream_drain_deadline);
    if externally_terminated || control.termination_requested() {
        return Ok(());
    }
    persist_workflow_command_captures(
        repo_root,
        project_id,
        run,
        node_run,
        command,
        &resolved_args,
        cwd.display_path(),
        exit_status.code().unwrap_or(-1),
        timed_out,
        stdout_capture,
        stderr_capture,
        success_exit_codes,
        output_contract,
        parser_extraction,
        parser_render_text_path,
        &registration,
    )
}

#[allow(clippy::too_many_arguments)]
fn persist_workflow_command_captures(
    repo_root: &Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    command: &str,
    resolved_args: &[String],
    cwd: &Path,
    exit_code: i32,
    timed_out: bool,
    stdout_capture: BoundedCommandCapture,
    stderr_capture: BoundedCommandCapture,
    success_exit_codes: &[i32],
    output_contract: &WorkflowOutputContractDto,
    parser_extraction: WorkflowOutputExtractionDto,
    parser_render_text_path: Option<&str>,
    registration: &RunningCommandRegistration,
) -> CommandResult<()> {
    let stdout = String::from_utf8_lossy(&stdout_capture.bytes).to_string();
    let stderr = String::from_utf8_lossy(&stderr_capture.bytes).to_string();
    let (parsed, parse_error) = match parser_extraction {
        WorkflowOutputExtractionDto::GenericText => (JsonValue::String(stdout.clone()), None),
        WorkflowOutputExtractionDto::JsonObject | WorkflowOutputExtractionDto::JsonArray => {
            match serde_json::from_str::<JsonValue>(&stdout) {
                Ok(value) => (value, None),
                Err(error) => (JsonValue::Null, Some(error.to_string())),
            }
        }
    };
    let parser_shape_valid = match parser_extraction {
        WorkflowOutputExtractionDto::GenericText => true,
        WorkflowOutputExtractionDto::JsonObject => parsed.is_object(),
        WorkflowOutputExtractionDto::JsonArray => parsed.is_array(),
    };
    let ok = !timed_out
        && success_exit_codes.contains(&exit_code)
        && !stdout_capture.truncated
        && !stderr_capture.truncated
        && stdout_capture.read_error.is_none()
        && stderr_capture.read_error.is_none()
        && parse_error.is_none()
        && parser_shape_valid;
    let payload = json!({
        "status": if ok { "passed" } else { "failed" },
        "command": command,
        "args": resolved_args,
        "workingDirectory": cwd,
        "exitCode": exit_code,
        "timedOut": timed_out,
        "stdout": stdout,
        "stderr": stderr,
        "stdoutTruncated": stdout_capture.truncated,
        "stderrTruncated": stderr_capture.truncated,
        "stdoutDrainIncomplete": stdout_capture.drain_incomplete,
        "stderrDrainIncomplete": stderr_capture.drain_incomplete,
        "stdoutReadError": stdout_capture.read_error,
        "stderrReadError": stderr_capture.read_error,
        "parsed": parsed,
        "parseError": parse_error,
    });
    let json_schema = artifact_schema_for_output(&run.definition_snapshot, output_contract);
    let (validated_render_text, diagnostics) =
        match validate_workflow_artifact_payload(output_contract, json_schema, &payload) {
            Ok(result) => result,
            Err(error) => {
                fail_node_with_recoverable_error(
                    repo_root,
                    project_id,
                    run,
                    node_run,
                    "workflow_command_artifact_invalid",
                    &error.code,
                    &error.message,
                )?;
                return Ok(());
            }
        };
    let render_text = parser_render_text_path
        .and_then(|path| json_path_lookup(&payload, path))
        .and_then(JsonValue::as_str)
        .or(validated_render_text.as_deref())
        .or_else(|| payload.get("stdout").and_then(JsonValue::as_str))
        .map(ToOwned::to_owned);
    let event = json!({
        "nodeId": node_run.node_id,
        "command": command,
        "exitCode": exit_code,
        "timedOut": timed_out,
        "stdoutTruncated": stdout_capture.truncated,
        "stderrTruncated": stderr_capture.truncated,
        "stdoutDrainIncomplete": stdout_capture.drain_incomplete,
        "stderrDrainIncomplete": stderr_capture.drain_incomplete,
        "stdoutReadError": stdout_capture.read_error,
        "stderrReadError": stderr_capture.read_error,
        "parseError": payload.get("parseError"),
        "status": if ok { "passed" } else { "failed" },
        "validationStatus": "valid",
        "diagnostics": diagnostics.iter().map(|diagnostic| {
            json!({
                "code": diagnostic.code,
                "path": diagnostic.path,
                "message": diagnostic.message,
            })
        }).collect::<Vec<_>>(),
    });
    if registration.control.termination_requested() {
        return Ok(());
    }
    project_store::complete_workflow_command_node(
        repo_root,
        project_id,
        &project_store::WorkflowCommandCompletionRecord {
            run_id: run.id.clone(),
            node_run_id: node_run.id.clone(),
            artifact_type: output_contract.artifact_type.clone(),
            schema_version: output_contract.schema_version,
            payload,
            render_text,
            event,
            status: if ok {
                WorkflowNodeRunStatusDto::Succeeded
            } else {
                WorkflowNodeRunStatusDto::Failed
            },
            failure_class: (!ok)
                .then(|| {
                    if timed_out {
                        "workflow_command_timeout"
                    } else if stdout_capture.drain_incomplete || stderr_capture.drain_incomplete {
                        "workflow_command_output_incomplete"
                    } else if stdout_capture.read_error.is_some()
                        || stderr_capture.read_error.is_some()
                    {
                        "workflow_command_output_failed"
                    } else if stdout_capture.truncated || stderr_capture.truncated {
                        "workflow_command_output_truncated"
                    } else {
                        "workflow_command_failed"
                    }
                })
                .map(ToOwned::to_owned),
            owner_instance_id: registration.owner_instance_id.clone(),
            lease_token: registration.lease_token.clone(),
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn run_internal_git_status_node(
    repo_root: &Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    command: &str,
    resolved_args: &[String],
    timeout_seconds: u32,
    success_exit_codes: &[i32],
    output_contract: &WorkflowOutputContractDto,
    parser_extraction: WorkflowOutputExtractionDto,
    parser_render_text_path: Option<&str>,
    registration: RunningCommandRegistration,
    cwd: &OpenedWorkflowCommandWorkingDirectory,
) -> CommandResult<()> {
    #[cfg(not(unix))]
    {
        let _ = (
            repo_root,
            project_id,
            run,
            node_run,
            command,
            resolved_args,
            timeout_seconds,
            success_exit_codes,
            output_contract,
            parser_extraction,
            parser_render_text_path,
            registration,
            cwd,
        );
        return Err(CommandError::user_fixable(
            "workflow_command_secure_launch_unsupported",
            "Secure Workflow command execution is not available on this platform.",
        ));
    }

    #[cfg(unix)]
    {
        let (directory_guard, repository_path) = cwd.repository_access().map_err(|error| {
            CommandError::system_fault(
                "workflow_command_working_directory_invalid",
                format!("Xero could not retain the Workflow command directory: {error}"),
            )
        })?;
        let args = resolved_args.to_vec();
        let (sender, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("workflow-git-status".into())
            .spawn(move || {
                let result = read_internal_git_status(&repository_path, &directory_guard, &args);
                let _ = sender.send(result);
            })
            .map_err(|error| {
                CommandError::retryable(
                    "workflow_command_thread_spawn_failed",
                    format!("Xero could not start the in-process Git status worker: {error}"),
                )
            })?;

        let started = Instant::now();
        let timeout = StdDuration::from_secs(timeout_seconds.into());
        let result = loop {
            if registration.control.termination_requested() {
                return Ok(());
            }
            if started.elapsed() >= timeout {
                break None;
            }
            match receiver.recv_timeout(StdDuration::from_millis(25)) {
                Ok(result) => break Some(result),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    break Some(Err(
                        "The in-process Git status worker stopped without a result.".into(),
                    ));
                }
            }
        };

        let empty_capture = || BoundedCommandCapture {
            bytes: Vec::new(),
            truncated: false,
            drain_incomplete: false,
            read_error: None,
        };
        let (exit_code, timed_out, stdout_capture, stderr_capture) = match result {
            Some(Ok(stdout)) => (0, false, stdout, empty_capture()),
            Some(Err(error)) => {
                let stderr = read_bounded_command_stream(
                    std::io::Cursor::new(error.into_bytes()),
                    MAX_COMMAND_STREAM_CAPTURE_BYTES,
                )
                .map_err(|capture_error| {
                    CommandError::system_fault(
                        "workflow_command_output_failed",
                        format!("Xero could not capture Git status failure: {capture_error}"),
                    )
                })?;
                (1, false, empty_capture(), stderr)
            }
            None => (-1, true, empty_capture(), empty_capture()),
        };
        persist_workflow_command_captures(
            repo_root,
            project_id,
            run,
            node_run,
            command,
            resolved_args,
            cwd.display_path(),
            exit_code,
            timed_out,
            stdout_capture,
            stderr_capture,
            success_exit_codes,
            output_contract,
            parser_extraction,
            parser_render_text_path,
            &registration,
        )
    }
}

fn read_internal_git_status(
    repository_path: &Path,
    directory_guard: &File,
    args: &[String],
) -> Result<BoundedCommandCapture, String> {
    #[cfg(unix)]
    ensure_path_matches_open_directory(repository_path, directory_guard)
        .map_err(|error| format!("The pinned Git status directory changed: {error}"))?;
    let repository = git2::Repository::discover(repository_path)
        .map_err(|error| format!("Xero could not open the Git repository: {error}"))?;
    let include_untracked = !args
        .iter()
        .any(|argument| matches!(argument.as_str(), "--untracked-files=no" | "-uno"));
    let recurse_untracked = args
        .iter()
        .any(|argument| matches!(argument.as_str(), "--untracked-files=all" | "-uall"));
    let nul_terminated = args
        .iter()
        .any(|argument| matches!(argument.as_str(), "-z" | "--null"));
    let pathspecs = args
        .iter()
        .position(|argument| argument == "--")
        .map(|separator| &args[separator + 1..])
        .unwrap_or(&[]);

    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(include_untracked)
        .recurse_untracked_dirs(recurse_untracked)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .include_ignored(false)
        .include_unmodified(false)
        .exclude_submodules(true)
        .disable_pathspec_match(true);
    for pathspec in pathspecs {
        options.pathspec(pathspec.as_str());
    }
    let statuses = repository
        .statuses(Some(&mut options))
        .map_err(|error| format!("Xero could not read Git status: {error}"))?;
    let mut capture = BoundedCommandCapture {
        bytes: Vec::new(),
        truncated: false,
        drain_incomplete: false,
        read_error: None,
    };
    for entry in statuses.iter() {
        let Some(path) = entry.path() else {
            continue;
        };
        let status = entry.status();
        let code = git_status_porcelain_code(status);
        let rendered_path = render_git_status_path(path, nul_terminated);
        let terminator = if nul_terminated { "\0" } else { "\n" };
        let line = format!("{code} {rendered_path}{terminator}");
        let remaining = MAX_COMMAND_STREAM_CAPTURE_BYTES.saturating_sub(capture.bytes.len());
        if line.len() > remaining {
            capture
                .bytes
                .extend_from_slice(&line.as_bytes()[..remaining]);
            capture.truncated = true;
            break;
        }
        capture.bytes.extend_from_slice(line.as_bytes());
    }
    #[cfg(unix)]
    ensure_path_matches_open_directory(repository_path, directory_guard)
        .map_err(|error| format!("The pinned Git status directory changed: {error}"))?;
    Ok(capture)
}

fn git_status_porcelain_code(status: git2::Status) -> String {
    if status.contains(git2::Status::WT_NEW) {
        return "??".into();
    }
    let staged = if status.contains(git2::Status::CONFLICTED) {
        'U'
    } else if status.contains(git2::Status::INDEX_NEW) {
        'A'
    } else if status.contains(git2::Status::INDEX_MODIFIED) {
        'M'
    } else if status.contains(git2::Status::INDEX_DELETED) {
        'D'
    } else if status.contains(git2::Status::INDEX_RENAMED) {
        'R'
    } else if status.contains(git2::Status::INDEX_TYPECHANGE) {
        'T'
    } else {
        ' '
    };
    let unstaged = if status.contains(git2::Status::CONFLICTED) {
        'U'
    } else if status.contains(git2::Status::WT_MODIFIED) {
        'M'
    } else if status.contains(git2::Status::WT_DELETED) {
        'D'
    } else if status.contains(git2::Status::WT_RENAMED) {
        'R'
    } else if status.contains(git2::Status::WT_TYPECHANGE) {
        'T'
    } else {
        ' '
    };
    format!("{staged}{unstaged}")
}

fn render_git_status_path(path: &str, nul_terminated: bool) -> String {
    if nul_terminated
        || path
            .chars()
            .all(|character| !character.is_control() && !character.is_whitespace())
    {
        return path.into();
    }
    serde_json::to_string(path).unwrap_or_else(|_| "\"<unrenderable-path>\"".into())
}

fn run_subgraph_node(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    subgraph_id: &str,
    input_bindings: &[WorkflowInputBindingDto],
    output_contract: &WorkflowOutputContractDto,
) -> CommandResult<()> {
    let Some(subgraph) = run
        .definition_snapshot
        .subgraphs
        .iter()
        .find(|subgraph| subgraph.id == subgraph_id)
    else {
        return fail_node_with_recoverable_error(
            repo_root,
            project_id,
            run,
            node_run,
            "workflow_subgraph_missing",
            "workflow_subgraph_missing",
            &format!("Subgraph `{subgraph_id}` is missing from the Workflow snapshot."),
        );
    };
    let input_payload = resolve_input_bindings_payload(run, input_bindings)?;
    let child_node_id = namespaced_subgraph_node_id(&node_run.node_id, &subgraph.start_node_id);
    let child_node_type = find_node(&run.definition_snapshot, &child_node_id)
        .map(|node| node.node_type().as_str().to_owned())
        .ok_or_else(|| {
            CommandError::system_fault(
                "workflow_subgraph_start_node_missing",
                format!("Subgraph start node `{child_node_id}` is missing."),
            )
        })?;
    project_store::start_workflow_subgraph_atomically(
        repo_root,
        project_id,
        &project_store::WorkflowSubgraphStartRecord {
            run_id: run.id.clone(),
            parent_node_run_id: node_run.id.clone(),
            parent_node_id: node_run.node_id.clone(),
            subgraph_id: subgraph_id.to_owned(),
            input_artifact_type: SUBGRAPH_INPUT_ARTIFACT_TYPE.to_owned(),
            input_payload,
            output_artifact_type: output_contract.artifact_type.clone(),
            child_node_id,
            child_node_type,
        },
    )
    .map(|_| ())
}

fn complete_subgraph_terminal(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    terminal_node_run: &WorkflowRunNodeDto,
    terminal_status: WorkflowTerminalStatusDto,
    context: &SubgraphNodeContext<'_>,
) -> CommandResult<()> {
    let parent_node_run = run
        .nodes
        .iter()
        .filter(|node| {
            node.node_id == context.parent_node_id.as_str()
                && node.status == WorkflowNodeRunStatusDto::Running
        })
        .max_by_key(|node| node.attempt_number)
        .ok_or_else(|| {
            CommandError::system_fault(
                "workflow_subgraph_parent_missing",
                format!(
                    "Subgraph child `{}` completed but parent `{}` is not running.",
                    terminal_node_run.node_id, context.parent_node_id
                ),
            )
        })?;
    let WorkflowNodeDto::Subgraph {
        subgraph_id,
        output_contract,
        ..
    } = context.parent_node
    else {
        return Err(CommandError::system_fault(
            "workflow_subgraph_parent_invalid",
            format!(
                "Subgraph child `{}` is attached to non-subgraph parent `{}`.",
                terminal_node_run.node_id, context.parent_node_id
            ),
        ));
    };
    let child_prefix = format!("{}{}", context.parent_node_id, SUBGRAPH_NODE_SEPARATOR);
    let child_node_run_ids = run
        .nodes
        .iter()
        .filter(|node| node.node_id.starts_with(&child_prefix))
        .map(|node| node.id.clone())
        .collect::<Vec<_>>();
    let status = subgraph_status_for_terminal(terminal_status);
    let summary = format!(
        "Subgraph `{}` completed with `{}`.",
        subgraph_id,
        terminal_status.as_str()
    );
    let payload = if output_contract.extraction == WorkflowOutputExtractionDto::GenericText {
        json!({ "text": summary.clone() })
    } else {
        json!({
            "status": status,
            "subgraphId": subgraph_id,
            "summary": summary.clone(),
            "terminalNodeId": terminal_node_run.node_id,
            "terminalStatus": terminal_status.as_str(),
            "childNodeRunIds": child_node_run_ids,
        })
    };
    let json_schema = artifact_schema_for_output(&run.definition_snapshot, output_contract);
    let (render_text, diagnostics) =
        match validate_workflow_artifact_payload(output_contract, json_schema, &payload) {
            Ok(result) => result,
            Err(error) => {
                fail_node_with_recoverable_error(
                    repo_root,
                    project_id,
                    run,
                    parent_node_run,
                    "workflow_subgraph_artifact_invalid",
                    &error.code,
                    &error.message,
                )?;
                return Ok(());
            }
        };

    let (parent_status, parent_failure_class, pause_run) = match terminal_status {
        WorkflowTerminalStatusDto::Success => (WorkflowNodeRunStatusDto::Succeeded, None, false),
        WorkflowTerminalStatusDto::Failure => (
            WorkflowNodeRunStatusDto::Failed,
            Some("workflow_subgraph_failed".to_owned()),
            false,
        ),
        WorkflowTerminalStatusDto::Cancelled => (
            WorkflowNodeRunStatusDto::Cancelled,
            Some("workflow_subgraph_cancelled".to_owned()),
            false,
        ),
        WorkflowTerminalStatusDto::NeedsHuman => {
            (WorkflowNodeRunStatusDto::WaitingOnGate, None, true)
        }
    };
    let resolved_render_text = render_text
        .or_else(|| {
            payload
                .get("summary")
                .and_then(JsonValue::as_str)
                .map(ToOwned::to_owned)
        })
        .or(Some(summary));
    project_store::complete_workflow_subgraph_atomically(
        repo_root,
        project_id,
        &project_store::WorkflowSubgraphCompletionRecord {
            run_id: run.id.clone(),
            terminal_node_run_id: terminal_node_run.id.clone(),
            terminal_node_id: terminal_node_run.node_id.clone(),
            parent_node_run_id: parent_node_run.id.clone(),
            parent_node_id: context.parent_node_id.clone(),
            parent_status,
            parent_failure_class,
            pause_run,
            artifact_type: output_contract.artifact_type.clone(),
            schema_version: output_contract.schema_version,
            payload,
            render_text: resolved_render_text,
            edge_evidence: json!({
                "terminalStatus": terminal_status.as_str(),
                "subgraphId": subgraph_id,
            }),
            terminal_event: json!({
                "nodeId": terminal_node_run.node_id,
                "parentNodeId": context.parent_node_id.clone(),
                "subgraphId": subgraph_id,
                "terminalStatus": terminal_status.as_str(),
            }),
            parent_event: json!({
                "nodeId": context.parent_node_id.clone(),
                "subgraphId": subgraph_id,
                "terminalNodeId": terminal_node_run.node_id,
                "terminalStatus": terminal_status.as_str(),
                "status": status,
                "validationStatus": "valid",
                "diagnostics": diagnostics.iter().map(|diagnostic| {
                    json!({
                        "code": diagnostic.code,
                        "path": diagnostic.path,
                        "message": diagnostic.message,
                    })
                }).collect::<Vec<_>>(),
            }),
        },
    )
    .map(|_| ())
}

fn subgraph_status_for_terminal(terminal_status: WorkflowTerminalStatusDto) -> &'static str {
    match terminal_status {
        WorkflowTerminalStatusDto::Success => "succeeded",
        WorkflowTerminalStatusDto::Failure => "failed",
        WorkflowTerminalStatusDto::Cancelled => "cancelled",
        WorkflowTerminalStatusDto::NeedsHuman => "needs_human",
    }
}

fn fail_node_with_recoverable_error(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    event_type: &str,
    failure_class: &str,
    message: &str,
) -> CommandResult<()> {
    project_store::fail_workflow_node_with_event_atomically(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        &node_run.node_id,
        event_type,
        failure_class,
        message,
    )
    .map(|_| ())
}

fn compare_json_values_for_runtime(
    left: Option<&JsonValue>,
    right: Option<&JsonValue>,
) -> std::cmp::Ordering {
    match (left, right) {
        (Some(left), Some(right)) => match (left.as_f64(), right.as_f64()) {
            (Some(left), Some(right)) => left
                .partial_cmp(&right)
                .unwrap_or(std::cmp::Ordering::Equal),
            _ => template_value_to_string(left).cmp(&template_value_to_string(right)),
        },
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

fn controls_for_agent_ref(
    repo_root: &std::path::Path,
    definition: &WorkflowDefinitionDto,
    agent_ref: &AgentRefDto,
    run_overrides: Option<&WorkflowRunOverrideDto>,
) -> CommandResult<RuntimeRunControlInputDto> {
    let (runtime_agent_id, agent_definition_id, agent_definition_version) = match agent_ref {
        AgentRefDto::BuiltIn {
            runtime_agent_id,
            version,
        } => (*runtime_agent_id, None, Some(*version)),
        AgentRefDto::Custom {
            definition_id,
            version,
        } => {
            let selection =
                project_store::resolve_pinned_agent_definition_version_for_started_workflow(
                    repo_root,
                    definition_id,
                    *version,
                    crate::commands::default_runtime_agent_id(),
                )?;
            (
                selection.runtime_agent_id,
                Some(selection.definition_id),
                Some(selection.version),
            )
        }
    };
    let approval_mode: RuntimeRunApprovalModeDto = run_overrides
        .and_then(|overrides| overrides.approval_mode.clone())
        .or_else(|| definition.run_policy.approval_mode.clone())
        .unwrap_or_else(|| default_runtime_agent_approval_mode(&runtime_agent_id));
    Ok(RuntimeRunControlInputDto {
        runtime_agent_id,
        agent_definition_id,
        agent_definition_version,
        provider_profile_id: run_overrides
            .and_then(|overrides| overrides.provider_profile_id.clone())
            .or_else(|| definition.run_policy.default_provider_profile_id.clone()),
        model_id: run_overrides
            .and_then(|overrides| overrides.model_id.clone())
            .or_else(|| definition.run_policy.default_model_id.clone())
            .unwrap_or_default(),
        thinking_effort: None,
        approval_mode,
        plan_mode_required: run_overrides
            .map(|overrides| overrides.plan_mode_required)
            .unwrap_or(false),
        auto_compact_enabled: run_overrides
            .map(|overrides| overrides.auto_compact_enabled)
            .unwrap_or(true),
    })
}

fn direct_merge_targets_for_skipped_branch(
    run: &WorkflowRunDto,
    skipped_node_id: &str,
) -> CommandResult<Vec<(String, String)>> {
    let mut targets = BTreeMap::new();
    for edge in runtime_edges_from_node(&run.definition_snapshot, skipped_node_id) {
        let Some(target @ WorkflowNodeDto::Merge { .. }) =
            find_node(&run.definition_snapshot, &edge.to_node_id)
        else {
            continue;
        };
        targets.insert(
            edge.to_node_id.clone(),
            target.node_type().as_str().to_owned(),
        );
    }
    if targets.is_empty() {
        return Err(CommandError::user_fixable(
            "workflow_branch_skip_requires_merge_target",
            format!(
                "Workflow node `{skipped_node_id}` has no direct Merge target, so its branch cannot be skipped safely."
            ),
        ));
    }
    Ok(targets.into_iter().collect())
}

struct LoopTargetResolution {
    target_node_id: String,
    source_status: WorkflowNodeRunStatusDto,
}

fn loop_target_for_edge(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    edge: &WorkflowEdgeDto,
) -> CommandResult<LoopTargetResolution> {
    let Some(policy) = edge.loop_policy.as_ref() else {
        return Ok(LoopTargetResolution {
            target_node_id: edge.to_node_id.clone(),
            source_status: node_run.status,
        });
    };
    if let Some(previous) = run.loop_attempts.iter().find(|attempt| {
        attempt.loop_key == policy.loop_key
            && attempt.last_node_run_id.as_deref() == Some(node_run.id.as_str())
    }) {
        return Ok(LoopTargetResolution {
            target_node_id: if previous.exhausted {
                policy.on_exhausted.clone()
            } else {
                edge.to_node_id.clone()
            },
            source_status: node_run.status,
        });
    }
    if let Some(detector) = policy.stall_detector {
        if let Some(failure_class) = stall_failure_class_for_detector(run, node_run, detector) {
            if !project_store::compare_and_set_workflow_run_node(
                repo_root,
                project_id,
                &node_run.id,
                &[node_run.status],
                WorkflowNodeRunStatusDto::Stalled,
                None,
                None,
                Some(failure_class),
            )? {
                return Ok(LoopTargetResolution {
                    target_node_id: edge.to_node_id.clone(),
                    source_status: node_run.status,
                });
            }
            project_store::increment_workflow_loop_attempt(
                repo_root,
                project_id,
                &run.id,
                &policy.loop_key,
                &node_run.id,
                true,
            )?;
            project_store::insert_workflow_event(
                repo_root,
                project_id,
                &run.id,
                Some(&node_run.id),
                "workflow_node_stalled",
                &json!({
                    "nodeId": node_run.node_id,
                    "failureClass": failure_class,
                    "stallDetector": detector.as_str(),
                    "loopKey": policy.loop_key.as_str(),
                }),
            )?;
            insert_workflow_metric_event(
                repo_root,
                project_id,
                &run.id,
                Some(&node_run.id),
                "loop_exhaustion",
                &json!({
                    "loopKey": policy.loop_key.as_str(),
                    "stallDetector": detector.as_str(),
                    "failureClass": failure_class,
                    "onExhausted": policy.on_exhausted.as_str(),
                }),
            )?;
            return Ok(LoopTargetResolution {
                target_node_id: policy.on_exhausted.clone(),
                source_status: WorkflowNodeRunStatusDto::Stalled,
            });
        }
    }
    let current_attempts = run
        .loop_attempts
        .iter()
        .find(|attempt| attempt.loop_key == policy.loop_key)
        .map(|attempt| attempt.attempt_count)
        .unwrap_or(0);
    if current_attempts >= policy.max_attempts {
        project_store::increment_workflow_loop_attempt(
            repo_root,
            project_id,
            &run.id,
            &policy.loop_key,
            &node_run.id,
            true,
        )?;
        insert_workflow_metric_event(
            repo_root,
            project_id,
            &run.id,
            Some(&node_run.id),
            "loop_exhaustion",
            &json!({
                "loopKey": policy.loop_key.as_str(),
                "attemptCount": current_attempts.saturating_add(1),
                "maxAttempts": policy.max_attempts,
                "onExhausted": policy.on_exhausted.as_str(),
            }),
        )?;
        return Ok(LoopTargetResolution {
            target_node_id: policy.on_exhausted.clone(),
            source_status: node_run.status,
        });
    }
    project_store::increment_workflow_loop_attempt(
        repo_root,
        project_id,
        &run.id,
        &policy.loop_key,
        &node_run.id,
        false,
    )?;
    Ok(LoopTargetResolution {
        target_node_id: edge.to_node_id.clone(),
        source_status: node_run.status,
    })
}

fn stall_failure_class_for_detector(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    detector: WorkflowStallDetectorDto,
) -> Option<&'static str> {
    match detector {
        WorkflowStallDetectorDto::FindingCountNotDecreasing => {
            finding_count_not_decreasing(run, node_run).then_some("finding_count_not_decreasing")
        }
        WorkflowStallDetectorDto::SameFailureClassRepeated => {
            same_failure_class_repeated(run, node_run).then_some("same_failure_class_repeated")
        }
        WorkflowStallDetectorDto::NoArtifactProgress => {
            no_artifact_progress(run, node_run).then_some("no_artifact_progress")
        }
        WorkflowStallDetectorDto::RuntimeActivityTimeout => (node_run.failure_class.as_deref()
            == Some(RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS))
        .then_some(RUNTIME_ACTIVITY_TIMEOUT_FAILURE_CLASS),
        WorkflowStallDetectorDto::RetryLimitExceeded => (node_run.failure_class.as_deref()
            == Some("retry_limit_exceeded"))
        .then_some("retry_limit_exceeded"),
    }
}

fn same_failure_class_repeated(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    let Some(current_failure) = node_run.failure_class.as_deref() else {
        return false;
    };
    run.nodes
        .iter()
        .filter(|candidate| {
            candidate.node_id == node_run.node_id
                && candidate.attempt_number < node_run.attempt_number
        })
        .max_by_key(|candidate| candidate.attempt_number)
        .and_then(|candidate| candidate.failure_class.as_deref())
        == Some(current_failure)
}

fn no_artifact_progress(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    let Some(contract) = output_contract_for_node(&run.definition_snapshot, &node_run.node_id)
    else {
        return false;
    };
    contract.required && artifacts_for_node_run(run, &node_run.id).is_empty()
}

fn finding_count_not_decreasing(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    let Some(current_count) = latest_finding_count_for_node_run(run, &node_run.id) else {
        return false;
    };
    let Some(previous_count) = run
        .nodes
        .iter()
        .filter(|candidate| {
            candidate.node_id == node_run.node_id
                && candidate.attempt_number < node_run.attempt_number
        })
        .max_by_key(|candidate| candidate.attempt_number)
        .and_then(|candidate| latest_finding_count_for_node_run(run, &candidate.id))
    else {
        return false;
    };
    current_count >= previous_count
}

fn latest_finding_count_for_node_run(run: &WorkflowRunDto, node_run_id: &str) -> Option<f64> {
    artifacts_for_node_run(run, node_run_id)
        .into_iter()
        .rev()
        .find_map(|artifact| finding_count_in_value(&artifact.payload))
}

fn artifacts_for_node_run<'a>(
    run: &'a WorkflowRunDto,
    node_run_id: &str,
) -> Vec<&'a crate::commands::contracts::workflows::WorkflowArtifactRecordDto> {
    run.artifacts
        .iter()
        .filter(|artifact| artifact.producer_node_run_id == node_run_id)
        .collect()
}

fn finding_count_in_value(value: &JsonValue) -> Option<f64> {
    match value {
        JsonValue::Object(map) => {
            for key in [
                "high_count",
                "highCount",
                "finding_count",
                "findingCount",
                "findings_count",
                "findingsCount",
                "gap_count",
                "gapCount",
                "gaps_count",
                "gapsCount",
            ] {
                if let Some(count) = map.get(key).and_then(JsonValue::as_f64) {
                    return Some(count);
                }
            }
            map.values().find_map(finding_count_in_value)
        }
        JsonValue::Array(items) => items.iter().find_map(finding_count_in_value),
        _ => None,
    }
}

fn condition_context(run: &WorkflowRunDto) -> WorkflowConditionContext {
    let mut context = WorkflowConditionContext::default();
    let mut node_id_by_run_id = BTreeMap::new();
    for node in &run.nodes {
        context
            .node_statuses
            .insert(node.node_id.clone(), node.status);
        if let Some(failure_class) = node.failure_class.as_ref() {
            context
                .failure_classes
                .insert(node.node_id.clone(), failure_class.clone());
            context.latest_failure_class = Some(failure_class.clone());
        }
        node_id_by_run_id.insert(node.id.clone(), node.node_id.clone());
    }
    for artifact in &run.artifacts {
        if let Some(node_id) = node_id_by_run_id.get(&artifact.producer_node_run_id) {
            let reference = format!("{node_id}.{}", artifact.artifact_type);
            context
                .artifacts
                .insert(reference.clone(), artifact.payload.clone());
            if artifact.artifact_type.starts_with("state_")
                || artifact.artifact_type == "collection_item"
            {
                context
                    .state_values
                    .insert(reference, artifact.payload.clone());
            }
        }
    }
    for attempt in &run.loop_attempts {
        context
            .loop_attempts
            .insert(attempt.loop_key.clone(), attempt.attempt_count);
    }
    for decision in &run.gate_decisions {
        if let Some(node_id) = node_id_by_run_id.get(&decision.node_run_id) {
            context
                .human_decisions
                .insert(node_id.clone(), decision.decision.clone());
        }
    }
    context
}

fn encode_workflow_condition(
    condition: &crate::commands::contracts::workflows::WorkflowConditionDto,
) -> CommandResult<JsonValue> {
    serde_json::to_value(condition).map_err(|error| {
        CommandError::system_fault(
            "workflow_condition_encode_failed",
            format!("Xero could not encode Workflow condition: {error}"),
        )
    })
}

fn had_prior_unsuccessful_attempt(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    run.nodes.iter().any(|candidate| {
        candidate.node_id == node_run.node_id
            && candidate.attempt_number < node_run.attempt_number
            && (is_failed_status(candidate.status)
                || candidate.status == WorkflowNodeRunStatusDto::Skipped)
    })
}

fn insert_workflow_metric_event(
    repo_root: &std::path::Path,
    project_id: &str,
    run_id: &str,
    node_run_id: Option<&str>,
    metric: &str,
    fields: &JsonValue,
) -> CommandResult<()> {
    project_store::insert_workflow_event(
        repo_root,
        project_id,
        run_id,
        node_run_id,
        "workflow_metric_recorded",
        &json!({
            "metric": metric,
            "fields": fields,
        }),
    )
}

fn output_contract_for_node<'a>(
    definition: &'a WorkflowDefinitionDto,
    node_id: &str,
) -> Option<&'a WorkflowOutputContractDto> {
    find_node(definition, node_id).and_then(WorkflowNodeDto::output_contract)
}

fn artifact_schema_for_output<'a>(
    definition: &'a WorkflowDefinitionDto,
    output_contract: &WorkflowOutputContractDto,
) -> Option<&'a JsonValue> {
    definition
        .artifact_contracts
        .iter()
        .find(|contract| {
            contract.artifact_type == output_contract.artifact_type
                && contract.schema_version == output_contract.schema_version
        })
        .and_then(|contract| contract.json_schema.as_ref())
}

#[derive(Debug, Clone)]
struct SubgraphNodeContext<'a> {
    parent_node_id: String,
    local_node_id: String,
    parent_node: &'a WorkflowNodeDto,
    subgraph: &'a WorkflowSubgraphDto,
}

fn find_node<'a>(
    definition: &'a WorkflowDefinitionDto,
    node_id: &str,
) -> Option<&'a WorkflowNodeDto> {
    if let Some(node) = definition.nodes.iter().find(|node| node.id() == node_id) {
        return Some(node);
    }
    let context = subgraph_context_for_node_id(definition, node_id)?;
    context
        .subgraph
        .nodes
        .iter()
        .find(|node| node.id() == context.local_node_id.as_str())
}

fn find_subgraph<'a>(
    definition: &'a WorkflowDefinitionDto,
    subgraph_id: &str,
) -> Option<&'a WorkflowSubgraphDto> {
    definition
        .subgraphs
        .iter()
        .find(|subgraph| subgraph.id == subgraph_id)
}

fn subgraph_context_for_node_id<'a>(
    definition: &'a WorkflowDefinitionDto,
    node_id: &str,
) -> Option<SubgraphNodeContext<'a>> {
    let (parent_node_id, local_node_id) = node_id.rsplit_once(SUBGRAPH_NODE_SEPARATOR)?;
    let parent_node = find_node(definition, parent_node_id)?;
    let WorkflowNodeDto::Subgraph { subgraph_id, .. } = parent_node else {
        return None;
    };
    let subgraph = find_subgraph(definition, subgraph_id)?;
    if !subgraph.nodes.iter().any(|node| node.id() == local_node_id) {
        return None;
    }
    Some(SubgraphNodeContext {
        parent_node_id: parent_node_id.to_string(),
        local_node_id: local_node_id.to_string(),
        parent_node,
        subgraph,
    })
}

fn namespaced_subgraph_node_id(parent_node_id: &str, local_node_id: &str) -> String {
    format!("{parent_node_id}{SUBGRAPH_NODE_SEPARATOR}{local_node_id}")
}

fn runtime_edges_from_node(
    definition: &WorkflowDefinitionDto,
    node_id: &str,
) -> Vec<WorkflowEdgeDto> {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return definition
            .edges
            .iter()
            .filter(|edge| edge.from_node_id == node_id)
            .cloned()
            .collect();
    };
    context
        .subgraph
        .edges
        .iter()
        .filter(|edge| edge.from_node_id == context.local_node_id.as_str())
        .map(|edge| namespace_subgraph_edge(edge, &context))
        .collect()
}

fn runtime_incoming_source_ids(definition: &WorkflowDefinitionDto, node_id: &str) -> Vec<String> {
    let Some(context) = subgraph_context_for_node_id(definition, node_id) else {
        return definition
            .edges
            .iter()
            .filter(|edge| edge.to_node_id == node_id)
            .map(|edge| edge.from_node_id.clone())
            .collect();
    };
    context
        .subgraph
        .edges
        .iter()
        .filter(|edge| edge.to_node_id == context.local_node_id.as_str())
        .map(|edge| namespaced_subgraph_node_id(&context.parent_node_id, &edge.from_node_id))
        .collect()
}

fn namespace_subgraph_edge(
    edge: &WorkflowEdgeDto,
    context: &SubgraphNodeContext<'_>,
) -> WorkflowEdgeDto {
    let mut edge = edge.clone();
    edge.id = format!("{}{}", context.parent_node_id, SUBGRAPH_NODE_SEPARATOR) + &edge.id;
    edge.from_node_id = namespaced_subgraph_node_id(&context.parent_node_id, &edge.from_node_id);
    edge.to_node_id =
        namespace_node_ref(&context.parent_node_id, context.subgraph, &edge.to_node_id);
    edge.condition =
        namespace_condition(&edge.condition, &context.parent_node_id, context.subgraph);
    if let Some(policy) = edge.loop_policy.as_mut() {
        policy.loop_key = namespace_loop_key(&context.parent_node_id, &policy.loop_key);
        policy.on_exhausted = namespace_node_ref(
            &context.parent_node_id,
            context.subgraph,
            &policy.on_exhausted,
        );
        policy.selected_artifact_refs = policy
            .selected_artifact_refs
            .iter()
            .map(|artifact_ref| {
                namespace_artifact_ref(&context.parent_node_id, context.subgraph, artifact_ref)
            })
            .collect();
    }
    edge
}

fn namespace_condition(
    condition: &WorkflowConditionDto,
    parent_node_id: &str,
    subgraph: &WorkflowSubgraphDto,
) -> WorkflowConditionDto {
    match condition {
        WorkflowConditionDto::Always => WorkflowConditionDto::Always,
        WorkflowConditionDto::All { conditions } => WorkflowConditionDto::All {
            conditions: conditions
                .iter()
                .map(|condition| namespace_condition(condition, parent_node_id, subgraph))
                .collect(),
        },
        WorkflowConditionDto::Any { conditions } => WorkflowConditionDto::Any {
            conditions: conditions
                .iter()
                .map(|condition| namespace_condition(condition, parent_node_id, subgraph))
                .collect(),
        },
        WorkflowConditionDto::Not { condition } => WorkflowConditionDto::Not {
            condition: Box::new(namespace_condition(condition, parent_node_id, subgraph)),
        },
        WorkflowConditionDto::NodeStatus { node_id, status } => WorkflowConditionDto::NodeStatus {
            node_id: namespace_node_ref(parent_node_id, subgraph, node_id),
            status: *status,
        },
        WorkflowConditionDto::ArtifactExists { artifact_ref } => {
            WorkflowConditionDto::ArtifactExists {
                artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            }
        }
        WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref,
            path,
            value,
        } => WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            path: path.clone(),
            value: value.clone(),
        },
        WorkflowConditionDto::ArtifactFieldIn {
            artifact_ref,
            path,
            values,
        } => WorkflowConditionDto::ArtifactFieldIn {
            artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            path: path.clone(),
            values: values.clone(),
        },
        WorkflowConditionDto::ArtifactFieldNumberCompare {
            artifact_ref,
            path,
            operator,
            value,
        } => WorkflowConditionDto::ArtifactFieldNumberCompare {
            artifact_ref: namespace_artifact_ref(parent_node_id, subgraph, artifact_ref),
            path: path.clone(),
            operator: *operator,
            value: *value,
        },
        WorkflowConditionDto::FailureClassIs {
            node_id,
            failure_class,
        } => WorkflowConditionDto::FailureClassIs {
            node_id: node_id
                .as_deref()
                .map(|node_id| namespace_node_ref(parent_node_id, subgraph, node_id)),
            failure_class: failure_class.clone(),
        },
        WorkflowConditionDto::LoopAttemptLt { loop_key, value } => {
            WorkflowConditionDto::LoopAttemptLt {
                loop_key: namespace_loop_key(parent_node_id, loop_key),
                value: *value,
            }
        }
        WorkflowConditionDto::LoopAttemptGte { loop_key, value } => {
            WorkflowConditionDto::LoopAttemptGte {
                loop_key: namespace_loop_key(parent_node_id, loop_key),
                value: *value,
            }
        }
        WorkflowConditionDto::HumanDecisionIs {
            checkpoint_node_id,
            decision,
        } => WorkflowConditionDto::HumanDecisionIs {
            checkpoint_node_id: namespace_node_ref(parent_node_id, subgraph, checkpoint_node_id),
            decision: decision.clone(),
        },
        WorkflowConditionDto::StateFieldEquals {
            state_ref,
            path,
            value,
        } => WorkflowConditionDto::StateFieldEquals {
            state_ref: namespace_artifact_ref(parent_node_id, subgraph, state_ref),
            path: path.clone(),
            value: value.clone(),
        },
        WorkflowConditionDto::StateCollectionCountCompare {
            state_ref,
            operator,
            value,
        } => WorkflowConditionDto::StateCollectionCountCompare {
            state_ref: namespace_artifact_ref(parent_node_id, subgraph, state_ref),
            operator: *operator,
            value: *value,
        },
    }
}

fn next_attempt_for_node(run: &WorkflowRunDto, node_id: &str) -> u32 {
    run.nodes
        .iter()
        .filter(|node| node.node_id == node_id)
        .map(|node| node.attempt_number)
        .max()
        .map(|attempt| attempt.saturating_add(1))
        .unwrap_or(0)
}

fn pause_at_checkpoint(
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    reason: &str,
) -> CommandResult<()> {
    project_store::pause_workflow_checkpoint_atomically(
        repo_root,
        project_id,
        &run.id,
        &node_run.id,
        &node_run.node_id,
        reason,
    )
    .map(|_| ())
}

fn complete_for_terminal(
    state: &DesktopState,
    repo_root: &std::path::Path,
    project_id: &str,
    run: &WorkflowRunDto,
    terminal_node_run: &WorkflowRunNodeDto,
    terminal_status: WorkflowTerminalStatusDto,
) -> CommandResult<()> {
    for node_run in run
        .nodes
        .iter()
        .filter(|node| node.id != terminal_node_run.id)
    {
        if matches!(
            find_node(&run.definition_snapshot, &node_run.node_id),
            Some(WorkflowNodeDto::Agent { .. } | WorkflowNodeDto::Command { .. })
        ) {
            terminate_workflow_node_execution(state, repo_root, project_id, run, node_run)?;
        }
    }
    let run_status = match terminal_status {
        WorkflowTerminalStatusDto::Success => WorkflowRunStatusDto::Completed,
        WorkflowTerminalStatusDto::Failure => WorkflowRunStatusDto::Failed,
        WorkflowTerminalStatusDto::Cancelled => WorkflowRunStatusDto::Cancelled,
        WorkflowTerminalStatusDto::NeedsHuman => WorkflowRunStatusDto::Paused,
    };
    project_store::complete_workflow_terminal_atomically(
        repo_root,
        project_id,
        &run.id,
        &terminal_node_run.id,
        run_status,
        terminal_status,
    )?;
    Ok(())
}

fn is_terminal_run(status: WorkflowRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowRunStatusDto::Completed
            | WorkflowRunStatusDto::Failed
            | WorkflowRunStatusDto::Cancelled
    )
}

fn edge_applies_to_node_status(
    edge_type: WorkflowEdgeTypeDto,
    node_status: WorkflowNodeRunStatusDto,
) -> bool {
    match edge_type {
        WorkflowEdgeTypeDto::Success => node_status == WorkflowNodeRunStatusDto::Succeeded,
        WorkflowEdgeTypeDto::Failure => matches!(
            node_status,
            WorkflowNodeRunStatusDto::Failed
                | WorkflowNodeRunStatusDto::Stalled
                | WorkflowNodeRunStatusDto::Cancelled
        ),
        WorkflowEdgeTypeDto::Recovery => matches!(
            node_status,
            WorkflowNodeRunStatusDto::Failed | WorkflowNodeRunStatusDto::Stalled
        ),
        WorkflowEdgeTypeDto::Conditional
        | WorkflowEdgeTypeDto::Loop
        | WorkflowEdgeTypeDto::ManualOverride => true,
    }
}

fn default_edge_specificity(edge_type: WorkflowEdgeTypeDto) -> u8 {
    match edge_type {
        WorkflowEdgeTypeDto::Success
        | WorkflowEdgeTypeDto::Failure
        | WorkflowEdgeTypeDto::Recovery => 1,
        WorkflowEdgeTypeDto::Conditional
        | WorkflowEdgeTypeDto::Loop
        | WorkflowEdgeTypeDto::ManualOverride => 0,
    }
}

fn routes_single_match(node: &WorkflowNodeDto) -> bool {
    matches!(
        node,
        WorkflowNodeDto::Router { .. }
            | WorkflowNodeDto::Gate { .. }
            | WorkflowNodeDto::HumanCheckpoint { .. }
    )
}

fn has_routed_node_run(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto) -> bool {
    if has_control_event_after_completion(run, node_run, "workflow_node_retry_requested") {
        return true;
    }
    has_control_event_after_completion(run, node_run, "workflow_node_routed")
}

fn has_node_event(run: &WorkflowRunDto, node_run: &WorkflowRunNodeDto, event_type: &str) -> bool {
    run.events.iter().any(|event| {
        event.node_run_id.as_deref() == Some(node_run.id.as_str()) && event.event_type == event_type
    })
}

fn has_metric_event_for_node(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    metric: &str,
) -> bool {
    run.events.iter().any(|event| {
        event.node_run_id.as_deref() == Some(node_run.id.as_str())
            && event.event_type == "workflow_metric_recorded"
            && event.event.get("metric").and_then(JsonValue::as_str) == Some(metric)
    })
}

fn has_control_event_after_completion(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    event_type: &str,
) -> bool {
    run.events.iter().any(|event| {
        event.node_run_id.as_deref() == Some(node_run.id.as_str())
            && event.event_type == event_type
            && node_run
                .completed_at
                .as_ref()
                .map(|completed_at| event.created_at >= *completed_at)
                .unwrap_or(true)
    })
}

fn is_retryable_node_status(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Stalled
            | WorkflowNodeRunStatusDto::Skipped
            | WorkflowNodeRunStatusDto::Cancelled
    )
}

fn is_skippable_node_status(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Pending
            | WorkflowNodeRunStatusDto::Eligible
            | WorkflowNodeRunStatusDto::Starting
            | WorkflowNodeRunStatusDto::Running
            | WorkflowNodeRunStatusDto::WaitingOnGate
    )
}

#[cfg(test)]
#[derive(Debug, Default, PartialEq, Eq)]
struct WorkflowEventReplaySummary {
    edge_evaluations: usize,
    node_start_requests: usize,
    resource_conflict_waits: usize,
    loop_exhaustions: usize,
    checkpoint_pauses: usize,
    recovery_successes: usize,
}

#[cfg(test)]
fn replay_workflow_events(run: &WorkflowRunDto) -> WorkflowEventReplaySummary {
    let mut summary = WorkflowEventReplaySummary::default();
    for event in &run.events {
        match event.event_type.as_str() {
            "workflow_edge_evaluated" => summary.edge_evaluations += 1,
            "workflow_agent_start_requested" => summary.node_start_requests += 1,
            "workflow_resource_conflict_wait" => summary.resource_conflict_waits += 1,
            "workflow_metric_recorded" => {
                match event.event.get("metric").and_then(JsonValue::as_str) {
                    Some("loop_exhaustion") => summary.loop_exhaustions += 1,
                    Some("checkpoint_pause") => summary.checkpoint_pauses += 1,
                    Some("recovery_success") => summary.recovery_successes += 1,
                    _ => {}
                }
            }
            _ => {}
        }
    }
    summary
}

#[derive(Debug, PartialEq, Eq)]
enum MergeEvaluation {
    Waiting,
    Succeeded,
    Failed(&'static str),
}

fn evaluate_merge_node(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
    wait_policy: WorkflowMergeWaitPolicyDto,
    quorum: Option<u32>,
    fail_fast: bool,
) -> MergeEvaluation {
    let incoming_sources = runtime_incoming_source_ids(&run.definition_snapshot, &node_run.node_id)
        .into_iter()
        .filter(|source_node_id| merge_source_is_participant(run, node_run, source_node_id))
        .collect::<std::collections::BTreeSet<_>>();
    if incoming_sources.is_empty() {
        return MergeEvaluation::Succeeded;
    }

    let statuses = incoming_sources
        .iter()
        .filter_map(|node_id| latest_status_for_node(run, node_id))
        .collect::<Vec<_>>();
    let finished_count = statuses
        .iter()
        .filter(|status| is_finished_status(**status))
        .count();
    let succeeded_count = statuses
        .iter()
        .filter(|status| **status == WorkflowNodeRunStatusDto::Succeeded)
        .count();
    let skipped_count = statuses
        .iter()
        .filter(|status| **status == WorkflowNodeRunStatusDto::Skipped)
        .count();
    let failed_count = statuses
        .iter()
        .filter(|status| is_failed_status(**status))
        .count();
    let resolved_without_failure_count = succeeded_count + skipped_count;
    let expected_count = incoming_sources.len();

    if fail_fast && failed_count > 0 {
        return MergeEvaluation::Failed("merge_branch_failed");
    }

    match wait_policy {
        WorkflowMergeWaitPolicyDto::Any => {
            if succeeded_count > 0 {
                MergeEvaluation::Succeeded
            } else if finished_count == expected_count {
                MergeEvaluation::Failed("merge_no_successful_branch")
            } else {
                MergeEvaluation::Waiting
            }
        }
        WorkflowMergeWaitPolicyDto::Quorum => {
            let required = quorum.unwrap_or(expected_count as u32).max(1) as usize;
            if succeeded_count >= required {
                MergeEvaluation::Succeeded
            } else if finished_count == expected_count {
                MergeEvaluation::Failed("merge_quorum_not_met")
            } else {
                MergeEvaluation::Waiting
            }
        }
        WorkflowMergeWaitPolicyDto::FailFast => {
            if failed_count > 0 {
                MergeEvaluation::Failed("merge_branch_failed")
            } else if resolved_without_failure_count == expected_count && succeeded_count > 0 {
                MergeEvaluation::Succeeded
            } else if finished_count == expected_count {
                MergeEvaluation::Failed("merge_no_successful_branch")
            } else {
                MergeEvaluation::Waiting
            }
        }
        WorkflowMergeWaitPolicyDto::All => {
            if failed_count > 0 && finished_count == expected_count {
                MergeEvaluation::Failed("merge_branch_failed")
            } else if resolved_without_failure_count == expected_count && succeeded_count > 0 {
                MergeEvaluation::Succeeded
            } else if finished_count == expected_count {
                MergeEvaluation::Failed("merge_no_successful_branch")
            } else {
                MergeEvaluation::Waiting
            }
        }
    }
}

fn merge_source_is_participant(
    run: &WorkflowRunDto,
    merge_node_run: &WorkflowRunNodeDto,
    source_node_id: &str,
) -> bool {
    if let Some(source_run) = latest_node_run(run, source_node_id) {
        if is_retry_rewound_node_run(source_run) {
            return node_may_still_activate_from_another_branch(
                run,
                &merge_node_run.node_id,
                source_node_id,
            );
        }
        if source_run.status == WorkflowNodeRunStatusDto::Skipped {
            // Explicit branch skipping inserts the Merge directly and records
            // the skipped source as a resolved participant.
            return true;
        }
        if let Some(targets) = routed_target_node_ids(run, source_run) {
            if targets.contains(&merge_node_run.node_id) {
                return true;
            }
            return false;
        }
        // Once a direct incoming source has an actual run, it is a Merge
        // participant even when it failed on an edge typed `success`. Merge
        // failure policy must observe that failed branch; edge applicability
        // only determines whether routing creates the Merge in the first place.
        // A durable route to another target was handled above, and a
        // retry-rewound attempt was handled before that.
        let is_direct_incoming_source =
            runtime_edges_from_node(&run.definition_snapshot, source_node_id)
                .into_iter()
                .any(|edge| edge.to_node_id == merge_node_run.node_id);
        if is_direct_incoming_source {
            return true;
        }
    }

    // The source may not exist yet on an activated multi-hop branch. Walk
    // forward from every other activated node, honoring durable route choices
    // for attempts that already routed. This excludes a router's unselected
    // branch without dropping a sibling whose next hop has not been inserted.
    node_may_still_activate_from_another_branch(run, &merge_node_run.node_id, source_node_id)
}

fn routed_target_node_ids(
    run: &WorkflowRunDto,
    node_run: &WorkflowRunNodeDto,
) -> Option<BTreeSet<String>> {
    run.events
        .iter()
        .rev()
        .find(|event| {
            event.node_run_id.as_deref() == Some(node_run.id.as_str())
                && event.event_type == "workflow_node_routed"
        })
        .map(|event| {
            event
                .event
                .get("targetNodeIds")
                .and_then(JsonValue::as_array)
                .into_iter()
                .flatten()
                .filter_map(JsonValue::as_str)
                .map(ToOwned::to_owned)
                .collect()
        })
}

fn node_may_still_activate_from_another_branch(
    run: &WorkflowRunDto,
    merge_node_id: &str,
    target_node_id: &str,
) -> bool {
    let mut queue = run
        .nodes
        .iter()
        .filter(|node| node.node_id != target_node_id && node.node_id != merge_node_id)
        .map(|node| node.node_id.clone())
        .collect::<VecDeque<_>>();
    let mut visited = BTreeSet::new();

    while let Some(node_id) = queue.pop_front() {
        if !visited.insert(node_id.clone()) {
            continue;
        }
        for possible_target in possible_future_route_targets(run, &node_id) {
            if possible_target == target_node_id {
                return true;
            }
            if possible_target != merge_node_id && !visited.contains(&possible_target) {
                queue.push_back(possible_target);
            }
        }
    }
    false
}

fn possible_future_route_targets(run: &WorkflowRunDto, node_id: &str) -> BTreeSet<String> {
    let Some(node_run) = latest_node_run(run, node_id) else {
        return runtime_edges_from_node(&run.definition_snapshot, node_id)
            .into_iter()
            .map(|edge| edge.to_node_id)
            .collect();
    };
    if is_retry_rewound_node_run(node_run) {
        return BTreeSet::new();
    }

    if let Some(routed) = routed_target_node_ids(run, node_run) {
        return routed;
    }
    if node_run.status == WorkflowNodeRunStatusDto::Skipped {
        return BTreeSet::new();
    }
    runtime_edges_from_node(&run.definition_snapshot, node_id)
        .into_iter()
        .filter(|edge| {
            !is_finished_status(node_run.status)
                || edge_applies_to_node_status(edge.r#type, node_run.status)
        })
        .map(|edge| edge.to_node_id)
        .collect()
}

fn latest_node_run<'a>(run: &'a WorkflowRunDto, node_id: &str) -> Option<&'a WorkflowRunNodeDto> {
    run.nodes
        .iter()
        .filter(|node| node.node_id == node_id)
        .max_by_key(|node| node.attempt_number)
}

fn is_retry_rewound_node_run(node_run: &WorkflowRunNodeDto) -> bool {
    node_run.status == WorkflowNodeRunStatusDto::Cancelled
        && node_run.failure_class.as_deref() == Some("workflow_retry_rewind")
}

fn latest_status_for_node(run: &WorkflowRunDto, node_id: &str) -> Option<WorkflowNodeRunStatusDto> {
    latest_node_run(run, node_id)
        .filter(|node| !is_retry_rewound_node_run(node))
        .map(|node| node.status)
}

fn is_finished_status(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Succeeded
            | WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Stalled
            | WorkflowNodeRunStatusDto::Skipped
            | WorkflowNodeRunStatusDto::Cancelled
    )
}

fn is_failed_status(status: WorkflowNodeRunStatusDto) -> bool {
    matches!(
        status,
        WorkflowNodeRunStatusDto::Failed
            | WorkflowNodeRunStatusDto::Stalled
            | WorkflowNodeRunStatusDto::Cancelled
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::contracts::{
            runtime::RuntimeAgentIdDto,
            workflows::{
                WorkflowArtifactRecordDto, WorkflowCollectionLoopControlsDto, WorkflowConditionDto,
                WorkflowEventDto, WorkflowFailureClassificationPolicyDto, WorkflowLoopPolicyDto,
                WorkflowResourceConflictModeDto, WorkflowResourceConflictPolicyDto,
                WorkflowRunPolicyDto, WorkflowStallDetectorDto,
            },
        },
        db::{
            configure_connection, migrations::migrations, project_store,
            register_project_database_path_for_tests,
        },
    };
    use rusqlite::Connection;
    use tempfile::TempDir;

    const NOW: &str = "2026-01-01T00:00:00Z";

    fn terminal_node(id: &str) -> WorkflowNodeDto {
        WorkflowNodeDto::Terminal {
            id: id.into(),
            title: id.into(),
            description: String::new(),
            position: Default::default(),
            terminal_status: WorkflowTerminalStatusDto::Success,
        }
    }

    fn agent_node(id: &str, resource_scopes: Vec<String>) -> WorkflowNodeDto {
        WorkflowNodeDto::Agent {
            id: id.into(),
            title: id.into(),
            description: String::new(),
            position: Default::default(),
            agent_ref: AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                version: 2,
            },
            display_label: None,
            input_bindings: Vec::new(),
            output_contract: WorkflowOutputContractDto::default(),
            run_overrides: None,
            resource_scopes,
            failure_policy: Default::default(),
        }
    }

    fn merge_node() -> WorkflowNodeDto {
        WorkflowNodeDto::Merge {
            id: "merge".into(),
            title: "Merge".into(),
            description: String::new(),
            position: Default::default(),
            wait_policy: WorkflowMergeWaitPolicyDto::All,
            quorum: None,
            fail_fast: false,
        }
    }

    fn edge(
        id: &str,
        from_node_id: &str,
        to_node_id: &str,
        edge_type: WorkflowEdgeTypeDto,
    ) -> WorkflowEdgeDto {
        WorkflowEdgeDto {
            id: id.into(),
            from_node_id: from_node_id.into(),
            to_node_id: to_node_id.into(),
            r#type: edge_type,
            label: String::new(),
            priority: 10,
            condition: WorkflowConditionDto::Always,
            loop_policy: None,
        }
    }

    fn definition_with_edges(edges: Vec<WorkflowEdgeDto>) -> WorkflowDefinitionDto {
        WorkflowDefinitionDto {
            schema: "xero.workflow_definition.v1".into(),
            id: "workflow-1".into(),
            project_id: "project-1".into(),
            name: "Workflow".into(),
            description: String::new(),
            version: 1,
            start_node_id: "source-a".into(),
            nodes: vec![
                terminal_node("source-a"),
                terminal_node("source-b"),
                terminal_node("source-c"),
                merge_node(),
                terminal_node("done"),
            ],
            edges,
            subgraphs: Vec::new(),
            artifact_contracts: Vec::new(),
            run_policy: WorkflowRunPolicyDto::default(),
            created_at: None,
            updated_at: None,
        }
    }

    fn node_run(
        node_id: &str,
        status: WorkflowNodeRunStatusDto,
        attempt_number: u32,
    ) -> WorkflowRunNodeDto {
        WorkflowRunNodeDto {
            id: format!("run-1:node:{node_id}:attempt:{attempt_number}"),
            workflow_run_id: "run-1".into(),
            node_id: node_id.into(),
            node_type: if node_id == "merge" {
                "merge".into()
            } else {
                "terminal".into()
            },
            status,
            attempt_number,
            runtime_run_id: None,
            agent_session_id: None,
            failure_class: None,
            started_at: None,
            updated_at: NOW.into(),
            completed_at: is_finished_status(status).then(|| NOW.into()),
            idempotency_key: format!("run-1:{node_id}:{attempt_number}"),
        }
    }

    fn artifact_for_node_run(node_run_id: &str, payload: JsonValue) -> WorkflowArtifactRecordDto {
        WorkflowArtifactRecordDto {
            id: format!("artifact-{node_run_id}"),
            workflow_run_id: "run-1".into(),
            producer_node_run_id: node_run_id.into(),
            artifact_type: "review_findings".into(),
            schema_version: 1,
            payload,
            render_text: None,
            created_at: NOW.into(),
        }
    }

    fn workflow_event(event_type: &str, event: JsonValue) -> WorkflowEventDto {
        WorkflowEventDto {
            id: format!("event-{event_type}"),
            workflow_run_id: "run-1".into(),
            node_run_id: None,
            event_type: event_type.into(),
            event,
            created_at: NOW.into(),
        }
    }

    fn routed_event(node_run: &WorkflowRunNodeDto, targets: &[&str]) -> WorkflowEventDto {
        WorkflowEventDto {
            id: format!("{}:routed", node_run.id),
            workflow_run_id: "run-1".into(),
            node_run_id: Some(node_run.id.clone()),
            event_type: "workflow_node_routed".into(),
            event: json!({
                "sourceNodeRunId": node_run.id,
                "targetNodeIds": targets,
            }),
            created_at: NOW.into(),
        }
    }

    fn run_with_nodes(
        edges: Vec<WorkflowEdgeDto>,
        nodes: Vec<WorkflowRunNodeDto>,
    ) -> WorkflowRunDto {
        run_with_definition(definition_with_edges(edges), nodes)
    }

    fn run_with_definition(
        definition: WorkflowDefinitionDto,
        nodes: Vec<WorkflowRunNodeDto>,
    ) -> WorkflowRunDto {
        WorkflowRunDto {
            id: "run-1".into(),
            project_id: "project-1".into(),
            workflow_version_id: "workflow-version-1".into(),
            workflow_id: definition.id.clone(),
            workflow_version_number: 1,
            status: WorkflowRunStatusDto::Running,
            terminal_status: None,
            definition_snapshot: definition,
            initial_input: None,
            started_at: NOW.into(),
            updated_at: NOW.into(),
            completed_at: None,
            cancellation_reason: None,
            nodes,
            edge_decisions: Vec::new(),
            artifacts: Vec::new(),
            gate_decisions: Vec::new(),
            loop_attempts: Vec::new(),
            events: Vec::new(),
        }
    }

    fn run_for_merge(
        sources: &[&str],
        node_statuses: Vec<(&str, WorkflowNodeRunStatusDto)>,
    ) -> (WorkflowRunDto, WorkflowRunNodeDto) {
        let merge_run = node_run("merge", WorkflowNodeRunStatusDto::Eligible, 0);
        let mut nodes = node_statuses
            .into_iter()
            .map(|(node_id, status)| node_run(node_id, status, 0))
            .collect::<Vec<_>>();
        nodes.push(merge_run.clone());

        let edges = sources
            .iter()
            .map(|source| {
                edge(
                    &format!("edge-{source}-merge"),
                    source,
                    "merge",
                    WorkflowEdgeTypeDto::Success,
                )
            })
            .collect::<Vec<_>>();

        (run_with_nodes(edges, nodes), merge_run)
    }

    #[test]
    fn input_bindings_payload_resolves_omitted_run_input_path_from_binding_name() {
        let mut run = run_with_nodes(Vec::new(), Vec::new());
        run.initial_input = Some(json!({
            "goal": "Ship it",
            "internal": "must not be copied into the goal binding"
        }));

        let payload = resolve_input_bindings_payload(
            &run,
            &[WorkflowInputBindingDto::RunInput {
                name: "goal".into(),
                required: true,
                path: None,
                prompt_label: Some("Goal".into()),
            }],
        )
        .expect("resolve input bindings");

        assert_eq!(payload, json!({ "goal": "Ship it" }));
    }

    #[test]
    fn merge_all_waits_until_every_incoming_source_finishes_successfully() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Running),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Waiting
        );

        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Succeeded),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Succeeded
        );

        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Failed),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Failed("merge_branch_failed")
        );
    }

    #[test]
    fn merge_any_succeeds_on_first_successful_branch() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Running),
            ],
        );

        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::Any,
                None,
                false,
            ),
            MergeEvaluation::Succeeded
        );
    }

    #[test]
    fn merge_all_treats_skipped_branches_as_resolved_not_successful() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Skipped),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Succeeded
        );

        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Skipped),
                ("source-b", WorkflowNodeRunStatusDto::Skipped),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Failed("merge_no_successful_branch")
        );
    }

    #[test]
    fn merge_excludes_a_router_branch_that_was_durably_not_selected() {
        let router = node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 0);
        let selected = node_run("source-b", WorkflowNodeRunStatusDto::Succeeded, 0);
        let merge_run = node_run("merge", WorkflowNodeRunStatusDto::Eligible, 0);
        let mut run = run_with_nodes(
            vec![
                edge(
                    "router-a",
                    "source-a",
                    "source-b",
                    WorkflowEdgeTypeDto::Success,
                ),
                edge(
                    "router-b",
                    "source-a",
                    "source-c",
                    WorkflowEdgeTypeDto::Success,
                ),
                edge("a-merge", "source-b", "merge", WorkflowEdgeTypeDto::Success),
                edge("b-merge", "source-c", "merge", WorkflowEdgeTypeDto::Success),
            ],
            vec![router.clone(), selected.clone(), merge_run.clone()],
        );
        run.events = vec![
            routed_event(&router, &["source-b"]),
            routed_event(&selected, &["merge"]),
        ];

        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Succeeded,
        );
    }

    #[test]
    fn merge_uses_only_the_latest_router_attempt_after_retry_changes_branches() {
        let old_router = node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 0);
        let new_router = node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 1);
        let selected = node_run("source-b", WorkflowNodeRunStatusDto::Succeeded, 0);
        let mut abandoned = node_run("source-c", WorkflowNodeRunStatusDto::Cancelled, 0);
        abandoned.failure_class = Some("workflow_retry_rewind".into());
        let merge_run = node_run("merge", WorkflowNodeRunStatusDto::Eligible, 0);
        let mut run = run_with_nodes(
            vec![
                edge(
                    "router-a",
                    "source-a",
                    "source-b",
                    WorkflowEdgeTypeDto::Success,
                ),
                edge(
                    "router-b",
                    "source-a",
                    "source-c",
                    WorkflowEdgeTypeDto::Success,
                ),
                edge("a-merge", "source-b", "merge", WorkflowEdgeTypeDto::Success),
                edge("b-merge", "source-c", "merge", WorkflowEdgeTypeDto::Success),
            ],
            vec![
                old_router.clone(),
                new_router.clone(),
                selected.clone(),
                abandoned,
                merge_run.clone(),
            ],
        );
        run.events = vec![
            routed_event(&old_router, &["source-c"]),
            routed_event(&new_router, &["source-b"]),
            routed_event(&selected, &["merge"]),
        ];

        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Succeeded,
        );
    }

    #[test]
    fn merge_waits_for_a_not_yet_inserted_source_on_an_activated_multihop_branch() {
        let split = node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 0);
        let fast = node_run("source-b", WorkflowNodeRunStatusDto::Succeeded, 0);
        let intermediate = node_run("done", WorkflowNodeRunStatusDto::Running, 0);
        let merge_run = node_run("merge", WorkflowNodeRunStatusDto::Eligible, 0);
        let mut run = run_with_nodes(
            vec![
                edge(
                    "split-fast",
                    "source-a",
                    "source-b",
                    WorkflowEdgeTypeDto::Success,
                ),
                edge(
                    "split-slow",
                    "source-a",
                    "done",
                    WorkflowEdgeTypeDto::Success,
                ),
                edge(
                    "fast-merge",
                    "source-b",
                    "merge",
                    WorkflowEdgeTypeDto::Success,
                ),
                edge(
                    "slow-next",
                    "done",
                    "source-c",
                    WorkflowEdgeTypeDto::Success,
                ),
                edge(
                    "slow-merge",
                    "source-c",
                    "merge",
                    WorkflowEdgeTypeDto::Success,
                ),
            ],
            vec![split.clone(), fast.clone(), intermediate, merge_run.clone()],
        );
        run.events = vec![
            routed_event(&split, &["source-b", "done"]),
            routed_event(&fast, &["merge"]),
        ];

        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                false,
            ),
            MergeEvaluation::Waiting,
        );
    }

    #[test]
    fn resource_conflict_policy_serializes_declared_scopes() {
        let mut definition = definition_with_edges(Vec::new());
        definition.nodes = vec![
            agent_node("agent-a", vec!["repo".into(), "src/lib.rs".into()]),
            agent_node("agent-b", vec!["src/lib.rs".into()]),
        ];
        definition.run_policy.concurrency_limit = 2;
        definition.run_policy.resource_conflict_policy = WorkflowResourceConflictPolicyDto {
            mode: WorkflowResourceConflictModeDto::SerializeConflicts,
            default_scopes: Vec::new(),
        };
        let eligible = node_run("agent-b", WorkflowNodeRunStatusDto::Eligible, 0);
        let run = run_with_definition(
            definition.clone(),
            vec![
                node_run("agent-a", WorkflowNodeRunStatusDto::Running, 0),
                eligible.clone(),
            ],
        );
        let conflict = resource_conflict_for_node(
            &run,
            &eligible,
            find_node(&definition, "agent-b").expect("agent-b exists"),
        )
        .expect("conflict exists");

        assert_eq!(conflict.node_id, "agent-a");
        assert_eq!(conflict.scopes, vec!["src/lib.rs".to_string()]);

        let mut allowed_definition = definition;
        allowed_definition.run_policy.resource_conflict_policy.mode =
            WorkflowResourceConflictModeDto::AllowConflicts;
        let allowed_run = run_with_definition(
            allowed_definition.clone(),
            vec![
                node_run("agent-a", WorkflowNodeRunStatusDto::Running, 0),
                eligible.clone(),
            ],
        );
        assert!(resource_conflict_for_node(
            &allowed_run,
            &eligible,
            find_node(&allowed_definition, "agent-b").expect("agent-b exists"),
        )
        .is_none());
    }

    #[test]
    fn merge_quorum_requires_configured_success_count() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b", "source-c"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Succeeded),
                ("source-c", WorkflowNodeRunStatusDto::Running),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::Quorum,
                Some(2),
                false,
            ),
            MergeEvaluation::Succeeded
        );

        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b", "source-c"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Succeeded),
                ("source-b", WorkflowNodeRunStatusDto::Failed),
                ("source-c", WorkflowNodeRunStatusDto::Cancelled),
            ],
        );
        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::Quorum,
                Some(2),
                false,
            ),
            MergeEvaluation::Failed("merge_quorum_not_met")
        );
    }

    #[test]
    fn merge_fail_fast_fails_before_all_sources_finish() {
        let (run, merge_run) = run_for_merge(
            &["source-a", "source-b"],
            vec![
                ("source-a", WorkflowNodeRunStatusDto::Failed),
                ("source-b", WorkflowNodeRunStatusDto::Running),
            ],
        );

        assert_eq!(
            evaluate_merge_node(
                &run,
                &merge_run,
                WorkflowMergeWaitPolicyDto::All,
                None,
                true,
            ),
            MergeEvaluation::Failed("merge_branch_failed")
        );
    }

    #[test]
    fn edge_status_routing_matches_terminal_status_semantics() {
        assert!(edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Success,
            WorkflowNodeRunStatusDto::Succeeded
        ));
        assert!(!edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Success,
            WorkflowNodeRunStatusDto::Failed
        ));

        for status in [
            WorkflowNodeRunStatusDto::Failed,
            WorkflowNodeRunStatusDto::Stalled,
            WorkflowNodeRunStatusDto::Cancelled,
        ] {
            assert!(edge_applies_to_node_status(
                WorkflowEdgeTypeDto::Failure,
                status
            ));
        }
        assert!(!edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Failure,
            WorkflowNodeRunStatusDto::Succeeded
        ));

        assert!(edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Recovery,
            WorkflowNodeRunStatusDto::Failed
        ));
        assert!(edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Recovery,
            WorkflowNodeRunStatusDto::Stalled
        ));
        assert!(!edge_applies_to_node_status(
            WorkflowEdgeTypeDto::Recovery,
            WorkflowNodeRunStatusDto::Cancelled
        ));

        for edge_type in [
            WorkflowEdgeTypeDto::Conditional,
            WorkflowEdgeTypeDto::Loop,
            WorkflowEdgeTypeDto::ManualOverride,
        ] {
            assert!(edge_applies_to_node_status(
                edge_type,
                WorkflowNodeRunStatusDto::Pending
            ));
        }

        assert!(
            default_edge_specificity(WorkflowEdgeTypeDto::Success)
                > default_edge_specificity(WorkflowEdgeTypeDto::Conditional)
        );
        assert!(
            default_edge_specificity(WorkflowEdgeTypeDto::Failure)
                > default_edge_specificity(WorkflowEdgeTypeDto::ManualOverride)
        );
    }

    #[test]
    fn activity_timeout_prefers_agent_policy_over_run_policy() {
        let mut definition = definition_with_edges(Vec::new());
        definition.run_policy.node_timeout_seconds = Some(60);
        definition.nodes.push(WorkflowNodeDto::Agent {
            id: "agent".into(),
            title: "Agent".into(),
            description: String::new(),
            position: Default::default(),
            agent_ref: AgentRefDto::BuiltIn {
                runtime_agent_id: RuntimeAgentIdDto::Engineer,
                version: 2,
            },
            display_label: None,
            input_bindings: Vec::new(),
            output_contract: WorkflowOutputContractDto::default(),
            run_overrides: None,
            resource_scopes: Vec::new(),
            failure_policy: WorkflowFailureClassificationPolicyDto {
                runtime_activity_timeout_seconds: Some(5),
                ..WorkflowFailureClassificationPolicyDto::default()
            },
        });

        assert_eq!(
            activity_timeout_seconds_for_node(&definition, "agent"),
            Some(5)
        );
        assert_eq!(
            activity_timeout_seconds_for_node(&definition, "source-a"),
            None
        );
    }

    #[test]
    fn stale_agent_activity_uses_latest_runtime_activity_timestamp() {
        let now = OffsetDateTime::parse("2026-01-01T00:10:00Z", &Rfc3339).expect("parse now");
        let recent_heartbeat = agent_run_record(
            "2026-01-01T00:00:00Z",
            Some("2026-01-01T00:09:00Z"),
            "2026-01-01T00:01:00Z",
        );
        assert_eq!(stale_agent_activity_at(&recent_heartbeat, 120, now), None);

        let stale_heartbeat = agent_run_record(
            "2026-01-01T00:00:00Z",
            Some("2026-01-01T00:07:00Z"),
            "2026-01-01T00:01:00Z",
        );
        assert_eq!(
            stale_agent_activity_at(&stale_heartbeat, 120, now),
            Some("2026-01-01T00:07:00Z")
        );
    }

    #[test]
    fn stall_detectors_classify_repeated_failures_missing_artifacts_and_flat_findings() {
        let first_failed = WorkflowRunNodeDto {
            failure_class: Some("tool_retry_limit".into()),
            ..node_run("source-a", WorkflowNodeRunStatusDto::Failed, 0)
        };
        let repeated_failed = WorkflowRunNodeDto {
            failure_class: Some("tool_retry_limit".into()),
            ..node_run("source-a", WorkflowNodeRunStatusDto::Failed, 1)
        };
        let run = run_with_nodes(Vec::new(), vec![first_failed, repeated_failed.clone()]);
        assert_eq!(
            stall_failure_class_for_detector(
                &run,
                &repeated_failed,
                WorkflowStallDetectorDto::SameFailureClassRepeated,
            ),
            Some("same_failure_class_repeated")
        );

        let missing_artifact = node_run("agent", WorkflowNodeRunStatusDto::Succeeded, 0);
        let mut definition = definition_with_edges(Vec::new());
        definition.nodes.push(agent_node("agent", Vec::new()));
        let run = run_with_definition(definition, vec![missing_artifact.clone()]);
        assert_eq!(
            stall_failure_class_for_detector(
                &run,
                &missing_artifact,
                WorkflowStallDetectorDto::NoArtifactProgress,
            ),
            Some("no_artifact_progress")
        );

        let previous_review = node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 0);
        let current_review = node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 1);
        let mut run = run_with_nodes(
            Vec::new(),
            vec![previous_review.clone(), current_review.clone()],
        );
        run.artifacts = vec![
            artifact_for_node_run(
                &previous_review.id,
                json!({ "findings": { "high_count": 2 } }),
            ),
            artifact_for_node_run(
                &current_review.id,
                json!({ "findings": { "high_count": 2 } }),
            ),
        ];
        assert_eq!(
            stall_failure_class_for_detector(
                &run,
                &current_review,
                WorkflowStallDetectorDto::FindingCountNotDecreasing,
            ),
            Some("finding_count_not_decreasing")
        );
    }

    fn agent_run_record(
        started_at: &str,
        last_heartbeat_at: Option<&str>,
        updated_at: &str,
    ) -> AgentRunRecord {
        AgentRunRecord {
            runtime_agent_id: RuntimeAgentIdDto::Engineer,
            agent_definition_id: "agent-definition".into(),
            agent_definition_version: 1,
            project_id: "project-1".into(),
            agent_session_id: "session-1".into(),
            run_id: "runtime-run-1".into(),
            trace_id: "trace-1".into(),
            lineage_kind: "root".into(),
            parent_run_id: None,
            parent_trace_id: None,
            parent_subagent_id: None,
            subagent_role: None,
            provider_id: "provider".into(),
            model_id: "model".into(),
            status: AgentRunStatus::Running,
            prompt: "prompt".into(),
            system_prompt: "system".into(),
            started_at: started_at.into(),
            last_heartbeat_at: last_heartbeat_at.map(ToOwned::to_owned),
            completed_at: None,
            cancelled_at: None,
            last_error: None,
            updated_at: updated_at.into(),
        }
    }

    #[test]
    fn latest_status_for_node_uses_highest_attempt() {
        let merge_run = node_run("merge", WorkflowNodeRunStatusDto::Eligible, 0);
        let run = run_with_nodes(
            vec![edge(
                "edge-source-a-merge",
                "source-a",
                "merge",
                WorkflowEdgeTypeDto::Success,
            )],
            vec![
                node_run("source-a", WorkflowNodeRunStatusDto::Failed, 0),
                node_run("source-a", WorkflowNodeRunStatusDto::Succeeded, 1),
                merge_run,
            ],
        );

        assert_eq!(
            latest_status_for_node(&run, "source-a"),
            Some(WorkflowNodeRunStatusDto::Succeeded)
        );
    }

    #[test]
    fn event_replay_reconstructs_workflow_observability_counts() {
        let mut run = run_with_nodes(Vec::new(), Vec::new());
        run.events = vec![
            workflow_event("workflow_edge_evaluated", json!({ "matched": true })),
            workflow_event(
                "workflow_agent_start_requested",
                json!({ "nodeId": "agent-a" }),
            ),
            workflow_event(
                "workflow_resource_conflict_wait",
                json!({ "nodeId": "agent-b" }),
            ),
            workflow_event(
                "workflow_metric_recorded",
                json!({ "metric": "loop_exhaustion" }),
            ),
            workflow_event(
                "workflow_metric_recorded",
                json!({ "metric": "checkpoint_pause" }),
            ),
            workflow_event(
                "workflow_metric_recorded",
                json!({ "metric": "recovery_success" }),
            ),
        ];

        assert_eq!(
            replay_workflow_events(&run),
            WorkflowEventReplaySummary {
                edge_evaluations: 1,
                node_start_requests: 1,
                resource_conflict_waits: 1,
                loop_exhaustions: 1,
                checkpoint_pauses: 1,
                recovery_successes: 1,
            }
        );
    }

    fn subgraph_output_contract() -> WorkflowOutputContractDto {
        WorkflowOutputContractDto {
            artifact_type: "subgraph_result".into(),
            schema_version: 1,
            extraction: WorkflowOutputExtractionDto::JsonObject,
            required: true,
            render_text_path: Some("$.summary".into()),
        }
    }

    fn subgraph_invocation_node(id: &str) -> WorkflowNodeDto {
        WorkflowNodeDto::Subgraph {
            id: id.into(),
            title: "Invoke subgraph".into(),
            description: String::new(),
            position: Default::default(),
            subgraph_id: "phase_flow".into(),
            input_bindings: vec![WorkflowInputBindingDto::RunInput {
                name: "goal".into(),
                required: true,
                path: Some("$.goal".into()),
                prompt_label: Some("Goal".into()),
            }],
            output_contract: subgraph_output_contract(),
        }
    }

    fn definition_with_subgraph(subgraph_edges: Vec<WorkflowEdgeDto>) -> WorkflowDefinitionDto {
        WorkflowDefinitionDto {
            schema: "xero.workflow_definition.v1".into(),
            id: "workflow-subgraph".into(),
            project_id: "project-1".into(),
            name: "Subgraph Workflow".into(),
            description: String::new(),
            version: 1,
            start_node_id: "invoke".into(),
            nodes: vec![subgraph_invocation_node("invoke"), terminal_node("done")],
            edges: vec![edge(
                "invoke-to-done",
                "invoke",
                "done",
                WorkflowEdgeTypeDto::Success,
            )],
            subgraphs: vec![WorkflowSubgraphDto {
                id: "phase_flow".into(),
                title: "Phase flow".into(),
                description: String::new(),
                start_node_id: "local_done".into(),
                nodes: vec![
                    terminal_node("producer"),
                    WorkflowNodeDto::Router {
                        id: "router".into(),
                        title: "Router".into(),
                        description: String::new(),
                        position: Default::default(),
                    },
                    terminal_node("local_done"),
                ],
                edges: subgraph_edges,
                input_bindings: vec![WorkflowInputBindingDto::RunInput {
                    name: "goal".into(),
                    required: true,
                    path: Some("$.goal".into()),
                    prompt_label: Some("Goal".into()),
                }],
                output_contract: subgraph_output_contract(),
            }],
            artifact_contracts: Vec::new(),
            run_policy: WorkflowRunPolicyDto::default(),
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn subgraph_node_schedules_child_run_and_completes_parent_from_local_terminal() {
        let temp = repo_with_database();
        let definition = definition_with_subgraph(Vec::new());
        let created = project_store::create_workflow_definition(temp.path(), &definition)
            .expect("create workflow");
        let run = project_store::create_workflow_run(
            temp.path(),
            "project-1",
            &created.id,
            Some(json!({ "goal": "ship subgraphs" })),
        )
        .expect("create run");
        project_store::update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run before scheduling subgraph");
        let parent_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "invoke",
            "subgraph",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "run-1:invoke:0",
        )
        .expect("insert parent node run");
        let loaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let WorkflowNodeDto::Subgraph {
            subgraph_id,
            input_bindings,
            output_contract,
            ..
        } = find_node(&loaded_run.definition_snapshot, "invoke").expect("invoke node")
        else {
            panic!("expected subgraph node");
        };

        run_subgraph_node(
            temp.path(),
            "project-1",
            &loaded_run,
            &parent_run,
            subgraph_id,
            input_bindings,
            output_contract,
        )
        .expect("start subgraph");

        let running_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload running run")
            .expect("run exists");
        let parent = running_run
            .nodes
            .iter()
            .find(|node| node.node_id == "invoke")
            .expect("parent exists");
        assert_eq!(parent.status, WorkflowNodeRunStatusDto::Running);
        assert!(running_run.artifacts.iter().any(|artifact| {
            artifact.producer_node_run_id == parent.id
                && artifact.artifact_type == SUBGRAPH_INPUT_ARTIFACT_TYPE
                && artifact.payload.get("goal").and_then(JsonValue::as_str)
                    == Some("ship subgraphs")
        }));

        let child = running_run
            .nodes
            .iter()
            .find(|node| node.node_id == "invoke::local_done")
            .cloned()
            .expect("child local terminal was scheduled");
        assert_eq!(child.status, WorkflowNodeRunStatusDto::Eligible);
        let context =
            subgraph_context_for_node_id(&running_run.definition_snapshot, &child.node_id)
                .expect("subgraph context");
        complete_subgraph_terminal(
            temp.path(),
            "project-1",
            &running_run,
            &child,
            WorkflowTerminalStatusDto::Success,
            &context,
        )
        .expect("complete subgraph");

        let finished_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload finished run")
            .expect("run exists");
        let parent = finished_run
            .nodes
            .iter()
            .find(|node| node.node_id == "invoke")
            .expect("parent exists");
        let child = finished_run
            .nodes
            .iter()
            .find(|node| node.node_id == "invoke::local_done")
            .expect("child exists");
        assert_eq!(parent.status, WorkflowNodeRunStatusDto::Succeeded);
        assert_eq!(child.status, WorkflowNodeRunStatusDto::Succeeded);
        let subgraph_artifact = finished_run
            .artifacts
            .iter()
            .find(|artifact| {
                artifact.producer_node_run_id == parent.id
                    && artifact.artifact_type == "subgraph_result"
            })
            .expect("subgraph result artifact");
        assert_eq!(
            subgraph_artifact
                .payload
                .get("status")
                .and_then(JsonValue::as_str),
            Some("succeeded")
        );
        assert_eq!(
            subgraph_artifact
                .payload
                .get("terminalNodeId")
                .and_then(JsonValue::as_str),
            Some("invoke::local_done")
        );
        assert!(finished_run.edge_decisions.iter().any(|decision| {
            decision.from_node_id == "invoke::local_done"
                && decision.to_node_id == "invoke"
                && decision.edge_id == "__subgraph_terminal__"
        }));
    }

    #[test]
    fn subgraph_edges_namespace_local_routes_conditions_and_loop_policy() {
        let mut pass_edge = edge(
            "local-pass",
            "router",
            "local_done",
            WorkflowEdgeTypeDto::Conditional,
        );
        pass_edge.condition = WorkflowConditionDto::ArtifactFieldEquals {
            artifact_ref: "producer.review_findings".into(),
            path: "$.status".into(),
            value: json!("passed"),
        };
        let mut retry_edge = edge(
            "local-retry",
            "router",
            "producer",
            WorkflowEdgeTypeDto::Loop,
        );
        retry_edge.loop_policy = Some(WorkflowLoopPolicyDto {
            loop_key: "review_retry".into(),
            max_attempts: 2,
            attempt_scope: Default::default(),
            carryover_policy: Default::default(),
            selected_artifact_refs: vec!["producer.review_findings".into()],
            reset_policy: Default::default(),
            stall_detector: None,
            on_exhausted: "local_done".into(),
        });
        let definition = definition_with_subgraph(vec![pass_edge, retry_edge]);

        let runtime_edges = runtime_edges_from_node(&definition, "invoke::router");
        let pass_edge = runtime_edges
            .iter()
            .find(|edge| edge.id == "invoke::local-pass")
            .expect("pass edge exists");
        assert_eq!(pass_edge.from_node_id, "invoke::router");
        assert_eq!(pass_edge.to_node_id, "invoke::local_done");
        assert_eq!(
            pass_edge.condition,
            WorkflowConditionDto::ArtifactFieldEquals {
                artifact_ref: "invoke::producer.review_findings".into(),
                path: "$.status".into(),
                value: json!("passed"),
            }
        );

        let retry_edge = runtime_edges
            .iter()
            .find(|edge| edge.id == "invoke::local-retry")
            .expect("retry edge exists");
        let policy = retry_edge.loop_policy.as_ref().expect("loop policy");
        assert_eq!(retry_edge.to_node_id, "invoke::producer");
        assert_eq!(policy.loop_key, "invoke::review_retry");
        assert_eq!(policy.on_exhausted, "invoke::local_done");
        assert_eq!(
            policy.selected_artifact_refs,
            vec!["invoke::producer.review_findings".to_string()]
        );
        assert_eq!(
            runtime_incoming_source_ids(&definition, "invoke::local_done"),
            vec!["invoke::router".to_string()]
        );
    }

    #[test]
    fn collection_control_selection_marks_partial_phase_runs() {
        let controls = WorkflowCollectionLoopControlsDto {
            from_input_path: Some("$.from".into()),
            to_input_path: Some("$.to".into()),
            only_input_path: Some("$.only".into()),
        };

        let selected = collection_control_selection(
            Some(&json!({
                "only": "2,2.1",
                "from": "2",
                "to": "3",
            })),
            &controls,
        );

        assert!(selected.has_selection);
        assert_eq!(selected.only_values, Some(vec![json!("2"), json!("2.1")]));
        assert_eq!(selected.from_value, Some(json!("2")));
        assert_eq!(selected.to_value, Some(json!("3")));

        let unselected = collection_control_selection(Some(&json!({ "goal": "ship" })), &controls);
        assert!(!unselected.has_selection);
    }

    fn repo_with_database() -> TempDir {
        let temp = TempDir::new().expect("create temp repo");
        let database_path = temp.path().join("state.db");
        register_project_database_path_for_tests(temp.path(), database_path.clone());
        let mut connection = Connection::open(&database_path).expect("open project db");
        configure_connection(&connection).expect("configure project db");
        migrations()
            .to_latest(&mut connection)
            .expect("migrate project db");
        connection
            .execute(
                r#"
                INSERT INTO projects (
                    id,
                    name,
                    description,
                    milestone,
                    total_phases,
                    completed_phases,
                    active_phase,
                    branch,
                    created_at,
                    updated_at
                )
                VALUES ('project-1', 'Project', '', '', 0, 0, 0, 'main', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')
                "#,
                [],
            )
            .expect("seed project");
        temp
    }

    #[test]
    fn runtime_command_allowlist_rejects_empty_and_mismatched_lists() {
        assert!(!workflow_command_is_allowlisted("git", &[]));
        assert!(!workflow_command_is_allowlisted(
            "git",
            &["cargo".into(), "pnpm".into()]
        ));
        assert!(workflow_command_is_allowlisted(
            "git",
            &["cargo".into(), "git".into()]
        ));
    }

    #[test]
    fn command_working_directory_must_resolve_inside_project_root() {
        let temp = TempDir::new().expect("create temp parent");
        let project_root = temp.path().join("project");
        let nested = project_root.join("nested");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&nested).expect("create nested project directory");
        std::fs::create_dir_all(&outside).expect("create outside directory");

        assert_eq!(
            resolve_workflow_command_working_directory(&project_root, Some("nested"))
                .expect("relative directory inside project"),
            nested.canonicalize().expect("canonical nested directory")
        );
        assert_eq!(
            resolve_workflow_command_working_directory(
                &project_root,
                Some(nested.to_str().expect("nested path is utf-8")),
            )
            .expect("absolute directory inside project"),
            nested.canonicalize().expect("canonical nested directory")
        );

        let traversal_error =
            resolve_workflow_command_working_directory(&project_root, Some("../outside"))
                .expect_err("parent traversal must be rejected");
        assert_eq!(
            traversal_error.code,
            "workflow_command_working_directory_outside_project"
        );
        let absolute_error = resolve_workflow_command_working_directory(
            &project_root,
            Some(outside.to_str().expect("outside path is utf-8")),
        )
        .expect_err("absolute outside directory must be rejected");
        assert_eq!(
            absolute_error.code,
            "workflow_command_working_directory_outside_project"
        );
    }

    #[cfg(unix)]
    #[test]
    fn command_working_directory_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("create temp parent");
        let project_root = temp.path().join("project");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&project_root).expect("create project directory");
        std::fs::create_dir_all(&outside).expect("create outside directory");
        symlink(&outside, project_root.join("escape")).expect("create escape symlink");

        let error = resolve_workflow_command_working_directory(&project_root, Some("escape"))
            .expect_err("symlink escape must be rejected");
        assert_eq!(
            error.code,
            "workflow_command_working_directory_outside_project"
        );
        assert!(open_workflow_command_working_directory(&project_root, Some("escape")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn opened_command_directory_cannot_be_redirected_by_a_late_symlink_swap() {
        use std::os::unix::fs::symlink;

        let temp = TempDir::new().expect("create temp parent");
        let project_root = temp.path().join("project");
        let nested = project_root.join("nested");
        let pinned = project_root.join("pinned");
        let outside = temp.path().join("outside");
        std::fs::create_dir_all(&nested).expect("create nested project directory");
        std::fs::create_dir_all(&outside).expect("create outside directory");
        std::fs::write(nested.join("inside-marker"), "inside").expect("write inside marker");
        std::fs::write(outside.join("outside-marker"), "outside").expect("write outside marker");

        let opened = open_workflow_command_working_directory(&project_root, Some("nested"))
            .expect("pin command directory");
        std::fs::rename(&nested, &pinned).expect("move original directory");
        symlink(&outside, &nested).expect("replace path with outside symlink");

        let mut process = Command::new("/bin/sh");
        process
            .arg("-c")
            .arg("test -f inside-marker && test ! -f outside-marker");
        opened
            .configure_process(&mut process)
            .expect("configure descriptor-relative cwd");
        assert!(process
            .status()
            .expect("run command in pinned directory")
            .success());
    }

    #[test]
    fn in_process_git_status_does_not_execute_repository_clean_filters() {
        let temp = TempDir::new().expect("create repository");
        let repository = git2::Repository::init(temp.path()).expect("initialize repository");
        std::fs::write(temp.path().join(".gitattributes"), "*.txt filter=hostile\n")
            .expect("write attributes");
        std::fs::write(temp.path().join("tracked.txt"), "before\n").expect("write tracked file");
        let mut index = repository.index().expect("open index");
        index
            .add_path(Path::new(".gitattributes"))
            .expect("stage attributes");
        index
            .add_path(Path::new("tracked.txt"))
            .expect("stage tracked file");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repository.find_tree(tree_id).expect("find tree");
        let signature =
            git2::Signature::now("Xero Test", "xero@example.com").expect("create signature");
        repository
            .commit(Some("HEAD"), &signature, &signature, "initial", &tree, &[])
            .expect("commit fixture");
        drop(tree);

        let marker = temp.path().join("external-filter-executed");
        let mut config = repository.config().expect("open repository config");
        config
            .set_str(
                "filter.hostile.clean",
                &format!("touch {}", marker.display()),
            )
            .expect("configure hostile clean filter");
        config
            .set_bool("filter.hostile.required", false)
            .expect("make filter optional");
        drop(config);
        std::fs::write(temp.path().join("tracked.txt"), "after\n").expect("modify tracked file");

        let opened = open_workflow_command_working_directory(temp.path(), None)
            .expect("pin repository directory");
        let (_directory_guard, descriptor_path) = opened
            .repository_access()
            .expect("open descriptor-backed repository path");
        let capture = read_internal_git_status(
            &descriptor_path,
            &_directory_guard,
            &["status".into(), "--short".into()],
        )
        .expect("read status in process");

        assert!(String::from_utf8_lossy(&capture.bytes).contains("tracked.txt"));
        assert!(
            !marker.exists(),
            "libgit2 status must not launch clean filters"
        );
    }

    #[test]
    fn command_stream_capture_is_bounded_while_draining_remaining_bytes() {
        let input = vec![b'x'; MAX_COMMAND_STREAM_CAPTURE_BYTES + 32_768];
        let capture = read_bounded_command_stream(
            std::io::Cursor::new(input),
            MAX_COMMAND_STREAM_CAPTURE_BYTES,
        )
        .expect("drain command stream");

        assert_eq!(capture.bytes.len(), MAX_COMMAND_STREAM_CAPTURE_BYTES);
        assert!(capture.truncated);
    }

    #[test]
    fn stdout_and_stderr_readers_capture_independently() {
        let stdout_reader = spawn_bounded_command_reader(std::io::Cursor::new(vec![
            b'o';
            MAX_COMMAND_STREAM_CAPTURE_BYTES
                + 1
        ]));
        let stderr_reader = spawn_bounded_command_reader(std::io::Cursor::new(vec![
            b'e';
            MAX_COMMAND_STREAM_CAPTURE_BYTES
                + 1
        ]));

        let deadline = Instant::now() + COMMAND_STREAM_DRAIN_TIMEOUT;
        let stdout = finish_bounded_command_reader(stdout_reader, deadline);
        let stderr = finish_bounded_command_reader(stderr_reader, deadline);
        assert_eq!(stdout.bytes.len(), MAX_COMMAND_STREAM_CAPTURE_BYTES);
        assert_eq!(stderr.bytes.len(), MAX_COMMAND_STREAM_CAPTURE_BYTES);
        assert!(stdout.truncated);
        assert!(stderr.truncated);
        assert!(!stdout.drain_incomplete);
        assert!(!stderr.drain_incomplete);
    }

    #[test]
    fn inherited_stream_handle_cannot_block_collection_past_deadline() {
        struct StalledAfterPrefixReader {
            prefix: Option<Vec<u8>>,
            release: Arc<(Mutex<bool>, Condvar)>,
            terminated: Arc<AtomicBool>,
        }

        impl Drop for StalledAfterPrefixReader {
            fn drop(&mut self) {
                self.terminated.store(true, Ordering::Release);
            }
        }

        impl Read for StalledAfterPrefixReader {
            fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
                if let Some(prefix) = self.prefix.take() {
                    buffer[..prefix.len()].copy_from_slice(&prefix);
                    return Ok(prefix.len());
                }

                let (released, wake) = &*self.release;
                let mut released = released
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                while !*released {
                    released = wake
                        .wait(released)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                }
                Ok(0)
            }
        }

        impl PollableCommandStream for StalledAfterPrefixReader {
            fn wait_until_readable(&self, _timeout: StdDuration) -> io::Result<bool> {
                if self.prefix.is_some() {
                    return Ok(true);
                }
                Ok(*self
                    .release
                    .0
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()))
            }
        }

        let release = Arc::new((Mutex::new(false), Condvar::new()));
        let terminated = Arc::new(AtomicBool::new(false));
        let reader = spawn_bounded_command_reader(StalledAfterPrefixReader {
            prefix: Some(b"partial".to_vec()),
            release: release.clone(),
            terminated: terminated.clone(),
        });
        let prefix_deadline = Instant::now() + StdDuration::from_secs(1);
        loop {
            let state = reader
                .state
                .0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if state.capture.bytes == b"partial" {
                break;
            }
            assert!(
                Instant::now() < prefix_deadline,
                "reader did not publish its partial capture"
            );
            drop(state);
            thread::sleep(StdDuration::from_millis(5));
        }

        let started = Instant::now();
        let capture =
            finish_bounded_command_reader(reader, Instant::now() + StdDuration::from_millis(40));
        assert!(started.elapsed() < StdDuration::from_secs(1));
        assert_eq!(capture.bytes, b"partial");
        assert!(capture.truncated);
        assert!(capture.drain_incomplete);
        assert!(capture.read_error.is_none());
        assert!(
            terminated.load(Ordering::Acquire),
            "bounded drain must join the retained-pipe reader"
        );

        let (released, wake) = &*release;
        *released
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        wake.notify_all();
    }

    #[cfg(unix)]
    #[test]
    fn command_pipes_are_drained_concurrently_without_unbounded_capture() {
        let mut process = Command::new("/bin/sh");
        process
            .arg("-c")
            .arg(
                "i=0; while [ \"$i\" -lt 20000 ]; do printf 'oooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooooo'; printf 'eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee' >&2; i=$((i + 1)); done",
            )
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        configure_process_tree_root(&mut process);
        let mut child = process.spawn().expect("spawn dual-stream command");
        let stdout_reader = spawn_bounded_command_reader(child.stdout.take().expect("stdout pipe"));
        let stderr_reader = spawn_bounded_command_reader(child.stderr.take().expect("stderr pipe"));

        let status = child.wait().expect("wait for dual-stream command");
        cleanup_process_group_after_root_exit(child.id());
        let deadline = Instant::now() + COMMAND_STREAM_DRAIN_TIMEOUT;
        let stdout = finish_bounded_command_reader(stdout_reader, deadline);
        let stderr = finish_bounded_command_reader(stderr_reader, deadline);

        assert!(status.success());
        assert_eq!(stdout.bytes.len(), MAX_COMMAND_STREAM_CAPTURE_BYTES);
        assert_eq!(stderr.bytes.len(), MAX_COMMAND_STREAM_CAPTURE_BYTES);
        assert!(stdout.truncated);
        assert!(stderr.truncated);
        assert!(!stdout.drain_incomplete);
        assert!(!stderr.drain_incomplete);
    }

    #[cfg(unix)]
    #[test]
    fn command_registry_requests_and_executes_process_tree_termination() {
        let registration =
            register_running_workflow_command("termination-project", "termination-run", "node-1")
                .expect("register running command");
        let mut process = Command::new("/bin/sh");
        process.arg("-c").arg("sleep 30");
        configure_process_tree_root(&mut process);
        let child = process.spawn().expect("spawn sleeping child");
        registration.control.attach_child(child);

        assert_eq!(
            terminate_running_workflow_commands(
                "termination-project",
                "termination-run",
                Some("node-1")
            ),
            1
        );
        assert!(registration.control.termination_requested());
        let status = registration
            .control
            .terminate()
            .expect("terminate sleeping process tree");
        assert!(!status.success());
        assert!(!registration.control.has_child());
        drop(registration);
        assert_eq!(
            terminate_running_workflow_commands(
                "termination-project",
                "termination-run",
                Some("node-1")
            ),
            0
        );
    }

    #[cfg(unix)]
    #[test]
    fn normally_reaped_command_is_removed_before_registration_drop() {
        let registration =
            register_running_workflow_command("reap-project", "reap-run", "reap-node")
                .expect("register command");
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("exit 0");
        configure_process_tree_root(&mut command);
        let child = command.spawn().expect("spawn command");
        register_process_tree_root(&child).expect("register process tree");
        registration.control.attach_child(child);
        let deadline = Instant::now() + StdDuration::from_secs(2);
        let status = loop {
            match registration.control.try_wait().expect("poll command") {
                Some(status) => break status,
                None => {
                    assert!(Instant::now() < deadline, "command did not exit");
                    thread::sleep(StdDuration::from_millis(10));
                }
            }
        };

        assert!(status.success());
        assert!(!registration.control.has_child());
        drop(registration);
    }

    #[cfg(unix)]
    fn assert_workflow_control_terminates_active_command(target_status: WorkflowNodeRunStatusDto) {
        let temp = repo_with_database();
        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(Vec::new()),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        project_store::update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start workflow run");
        let node_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            &format!("{}:control-command:0", run.id),
        )
        .expect("insert command node run");
        let mut registration =
            register_running_workflow_command("project-1", &run.id, &node_run.id)
                .expect("register command");
        assert!(project_store::claim_workflow_command_node_starting(
            temp.path(),
            "project-1",
            &run.id,
            &node_run.id,
            &registration.owner_instance_id,
            registration.owner_process_id,
            &registration.owner_process_birth_identity,
            &registration.lease_token,
            &crate::auth::now_timestamp(),
        )
        .expect("claim command lease"));
        registration
            .activate_persisted_lease(temp.path())
            .expect("start command heartbeat");
        let loaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");
        let control = registration.control.clone();
        let repo_root = temp.path().to_path_buf();
        let run_id = run.id.clone();
        let node_run_id = node_run.id.clone();
        let command_thread = thread::spawn(move || {
            run_command_node(
                &repo_root,
                "project-1",
                &loaded_run,
                &node_run,
                "/bin/sh",
                &["-c".into(), "sleep 30".into()],
                &["/bin/sh".into()],
                None,
                60,
                &[0],
                &WorkflowOutputContractDto::default(),
                WorkflowOutputExtractionDto::GenericText,
                None,
                registration,
            )
        });

        let attach_deadline = Instant::now() + StdDuration::from_secs(2);
        while control
            .child
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_none()
        {
            assert!(
                Instant::now() < attach_deadline,
                "command child was not attached"
            );
            thread::sleep(StdDuration::from_millis(10));
        }
        assert_eq!(
            terminate_running_workflow_commands("project-1", &run_id, Some(&node_run_id)),
            1
        );
        if target_status == WorkflowNodeRunStatusDto::Cancelled {
            project_store::request_workflow_run_cancellation(
                temp.path(),
                "project-1",
                &run_id,
                Some("test control"),
            )
            .expect("request workflow cancellation");
        } else {
            assert!(project_store::compare_and_set_workflow_run_node(
                temp.path(),
                "project-1",
                &node_run_id,
                &[
                    WorkflowNodeRunStatusDto::Starting,
                    WorkflowNodeRunStatusDto::Running,
                ],
                target_status,
                None,
                None,
                Some("test_control"),
            )
            .expect("apply workflow node control"));
        }
        command_thread
            .join()
            .expect("join command thread")
            .expect("command exits cleanly after control");
        if target_status == WorkflowNodeRunStatusDto::Cancelled {
            project_store::cancel_workflow_run_execution(
                temp.path(),
                "project-1",
                &run_id,
                Some("test control"),
            )
            .expect("finalize workflow cancellation");
        }

        let finished = project_store::get_workflow_run(temp.path(), "project-1", &run_id)
            .expect("reload controlled run")
            .expect("run exists");
        let finished_node = finished
            .nodes
            .iter()
            .find(|node| node.id == node_run_id)
            .expect("controlled node exists");
        assert_eq!(finished_node.status, target_status);
        assert!(!finished
            .events
            .iter()
            .any(|event| event.event_type == "workflow_command_completed"));
        assert!(!finished
            .artifacts
            .iter()
            .any(|artifact| artifact.producer_node_run_id == node_run_id));
    }

    #[cfg(unix)]
    #[test]
    fn cancel_skip_and_stall_terminate_active_command_without_late_completion() {
        for target_status in [
            WorkflowNodeRunStatusDto::Cancelled,
            WorkflowNodeRunStatusDto::Skipped,
            WorkflowNodeRunStatusDto::Stalled,
        ] {
            assert_workflow_control_terminates_active_command(target_status);
        }
    }

    #[test]
    fn live_registered_command_is_not_misclassified_as_interrupted() {
        let temp = repo_with_database();
        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(Vec::new()),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        let command_node_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Running,
            "run-1:live-command:0",
        )
        .expect("insert running command node run");
        let registration =
            register_running_workflow_command("project-1", &run.id, &command_node_run.id)
                .expect("register live command");
        let loaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");

        assert!(
            !reconcile_interrupted_command_nodes(temp.path(), "project-1", &loaded_run)
                .expect("reconcile live command")
        );
        drop(registration);
        let reloaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload run")
            .expect("run exists");
        let reloaded_node = reloaded_run
            .nodes
            .iter()
            .find(|node| node.id == command_node_run.id)
            .expect("command node exists");
        assert_eq!(reloaded_node.status, WorkflowNodeRunStatusDto::Running);
    }

    #[test]
    fn foreign_live_command_lease_is_not_misclassified_as_interrupted() {
        let temp = repo_with_database();
        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(Vec::new()),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        project_store::update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let node = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "run-1:foreign-command:0",
        )
        .expect("insert command node");
        assert!(project_store::claim_workflow_command_node_starting(
            temp.path(),
            "project-1",
            &run.id,
            &node.id,
            "foreign-app-instance",
            process::id(),
            &process_birth_identity(process::id()).expect("current process identity"),
            "foreign-lease",
            &crate::auth::now_timestamp(),
        )
        .expect("claim foreign lease"));
        let loaded = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");

        assert!(
            !reconcile_interrupted_command_nodes(temp.path(), "project-1", &loaded)
                .expect("preserve foreign live command")
        );
        let lease = project_store::load_workflow_command_lease(temp.path(), "project-1", &node.id)
            .expect("load lease")
            .expect("lease remains");
        assert_eq!(lease.owner_instance_id, "foreign-app-instance");
    }

    #[test]
    fn stale_heartbeat_does_not_steal_from_a_live_command_owner() {
        let temp = repo_with_database();
        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(Vec::new()),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        project_store::update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let node = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "run-1:expired-command:0",
        )
        .expect("insert command node");
        assert!(project_store::claim_workflow_command_node_starting(
            temp.path(),
            "project-1",
            &run.id,
            &node.id,
            "dead-app-instance",
            process::id(),
            &process_birth_identity(process::id()).expect("current process identity"),
            "expired-lease",
            "2020-01-01T00:00:00Z",
        )
        .expect("claim expired lease"));
        let loaded = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");

        assert!(
            !reconcile_interrupted_command_nodes(temp.path(), "project-1", &loaded)
                .expect("preserve the live owner")
        );
        let recovered = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload run")
            .expect("run exists");
        assert_eq!(
            recovered
                .nodes
                .iter()
                .find(|candidate| candidate.id == node.id)
                .expect("node exists")
                .status,
            WorkflowNodeRunStatusDto::Starting,
        );
        assert!(
            project_store::load_workflow_command_lease(temp.path(), "project-1", &node.id,)
                .expect("load lease")
                .is_some()
        );
    }

    #[cfg(unix)]
    #[test]
    fn orphan_cleanup_refuses_to_signal_a_reused_process_identity() {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("sleep 30");
        configure_process_tree_root(&mut command);
        let mut child = command.spawn().expect("spawn command");
        register_process_tree_root(&child).expect("register process tree");
        let child_id = child.id();
        let birth = workflow_command_process_birth_identity(child_id)
            .expect("capture command birth identity");

        assert!(!terminate_orphaned_workflow_command(
            child_id,
            "different-process-birth",
        ));
        assert!(child.try_wait().expect("poll command").is_none());
        assert!(terminate_orphaned_workflow_command(child_id, &birth));
        let _ = child.wait();
        cleanup_process_group_after_root_exit(child_id);
    }

    #[cfg(unix)]
    #[test]
    fn durable_control_transition_is_observed_by_owner_heartbeat() {
        let temp = repo_with_database();
        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(Vec::new()),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        project_store::update_workflow_run_status(
            temp.path(),
            "project-1",
            &run.id,
            WorkflowRunStatusDto::Running,
            None,
            None,
        )
        .expect("start run");
        let node = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Eligible,
            "run-1:heartbeat-command:0",
        )
        .expect("insert command node");
        let mut registration = register_running_workflow_command("project-1", &run.id, &node.id)
            .expect("register command");
        assert!(project_store::claim_workflow_command_node_starting(
            temp.path(),
            "project-1",
            &run.id,
            &node.id,
            &registration.owner_instance_id,
            registration.owner_process_id,
            &registration.owner_process_birth_identity,
            &registration.lease_token,
            &crate::auth::now_timestamp(),
        )
        .expect("claim lease"));
        registration
            .activate_persisted_lease(temp.path())
            .expect("start heartbeat");
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg("sleep 30");
        configure_process_tree_root(&mut command);
        let child = command.spawn().expect("spawn command");
        let child_id = child.id();
        let child_birth = workflow_command_process_birth_identity(child_id);
        assert!(project_store::attach_workflow_command_process(
            temp.path(),
            "project-1",
            &node.id,
            &registration.owner_instance_id,
            &registration.lease_token,
            child_id,
            child_birth.as_deref(),
            &crate::auth::now_timestamp(),
        )
        .expect("attach process"));
        registration.control.attach_child(child);

        assert!(project_store::compare_and_set_workflow_run_node(
            temp.path(),
            "project-1",
            &node.id,
            &[WorkflowNodeRunStatusDto::Starting],
            WorkflowNodeRunStatusDto::Stalled,
            None,
            None,
            Some("test_cross_process_control"),
        )
        .expect("persist remote control"));
        let deadline = Instant::now() + StdDuration::from_secs(5);
        while !registration.control.termination_requested() {
            assert!(
                Instant::now() < deadline,
                "heartbeat did not observe lease loss"
            );
            thread::sleep(StdDuration::from_millis(25));
        }
        drop(registration);
        let process_id = libc::pid_t::try_from(child_id).expect("child pid fits pid_t");
        let process_gone = unsafe { libc::kill(process_id, 0) } == -1
            && io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH);
        assert!(
            process_gone,
            "lease loss must terminate and reap the command"
        );
    }

    #[test]
    fn interrupted_command_is_stalled_instead_of_restarted() {
        let temp = repo_with_database();
        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(Vec::new()),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        let command_node_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "command",
            0,
            WorkflowNodeRunStatusDto::Starting,
            "run-1:command:0",
        )
        .expect("insert starting command node run");
        let loaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");

        assert!(
            reconcile_interrupted_command_nodes(temp.path(), "project-1", &loaded_run)
                .expect("reconcile interrupted command")
        );

        let reloaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload run")
            .expect("run exists");
        let reloaded_node = reloaded_run
            .nodes
            .iter()
            .find(|node| node.id == command_node_run.id)
            .expect("command node exists");
        assert_eq!(reloaded_node.status, WorkflowNodeRunStatusDto::Stalled);
        assert_eq!(
            reloaded_node.failure_class.as_deref(),
            Some(COMMAND_INTERRUPTED_FAILURE_CLASS)
        );
        assert_eq!(
            reloaded_run
                .events
                .iter()
                .filter(|event| event.event_type == "workflow_command_interrupted")
                .count(),
            1
        );
    }

    #[test]
    fn replaying_the_same_loop_source_does_not_consume_an_attempt() {
        let temp = repo_with_database();
        let mut retry_edge = edge(
            "edge-retry",
            "source-a",
            "source-b",
            WorkflowEdgeTypeDto::Loop,
        );
        retry_edge.loop_policy = Some(WorkflowLoopPolicyDto {
            loop_key: "retry".into(),
            max_attempts: 2,
            attempt_scope: Default::default(),
            carryover_policy: Default::default(),
            selected_artifact_refs: Vec::new(),
            reset_policy: Default::default(),
            stall_detector: None,
            on_exhausted: "done".into(),
        });

        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(vec![retry_edge.clone()]),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        let source_node_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "terminal",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "run-1:source-a:0",
        )
        .expect("insert source node run");
        let loaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");

        let first_target = loop_target_for_edge(
            temp.path(),
            "project-1",
            &loaded_run,
            &source_node_run,
            &retry_edge,
        )
        .expect("resolve first loop target");
        let after_first = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload after first route")
            .expect("run exists");
        let replay_target = loop_target_for_edge(
            temp.path(),
            "project-1",
            &after_first,
            &source_node_run,
            &retry_edge,
        )
        .expect("replay loop target");

        assert_eq!(first_target.target_node_id, "source-b");
        assert_eq!(replay_target.target_node_id, first_target.target_node_id);
        let after_replay = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload after replay")
            .expect("run exists");
        let retry_attempt = after_replay
            .loop_attempts
            .iter()
            .find(|attempt| attempt.loop_key == "retry")
            .expect("retry attempt exists");
        assert_eq!(retry_attempt.attempt_count, 1);
        assert!(!retry_attempt.exhausted);
    }

    #[test]
    fn exhausted_loop_routes_to_fallback_and_records_exhaustion() {
        let temp = repo_with_database();
        let mut retry_edge = edge(
            "edge-retry",
            "source-a",
            "source-b",
            WorkflowEdgeTypeDto::Loop,
        );
        retry_edge.loop_policy = Some(WorkflowLoopPolicyDto {
            loop_key: "retry".into(),
            max_attempts: 1,
            attempt_scope: Default::default(),
            carryover_policy: Default::default(),
            selected_artifact_refs: Vec::new(),
            reset_policy: Default::default(),
            stall_detector: None,
            on_exhausted: "done".into(),
        });

        let created = project_store::create_workflow_definition(
            temp.path(),
            &definition_with_edges(vec![retry_edge.clone()]),
        )
        .expect("create workflow");
        let run = project_store::create_workflow_run(temp.path(), "project-1", &created.id, None)
            .expect("create run");
        let prior_source_node_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "terminal",
            0,
            WorkflowNodeRunStatusDto::Succeeded,
            "run-1:source-a:0",
        )
        .expect("insert prior source node run");
        project_store::increment_workflow_loop_attempt(
            temp.path(),
            "project-1",
            &run.id,
            "retry",
            &prior_source_node_run.id,
            false,
        )
        .expect("seed first loop attempt");
        let source_node_run = project_store::insert_workflow_run_node(
            temp.path(),
            "project-1",
            &run.id,
            "source-a",
            "terminal",
            1,
            WorkflowNodeRunStatusDto::Succeeded,
            "run-1:source-a:1",
        )
        .expect("insert current source node run");
        let loaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("load run")
            .expect("run exists");

        let target = loop_target_for_edge(
            temp.path(),
            "project-1",
            &loaded_run,
            &source_node_run,
            &retry_edge,
        )
        .expect("resolve exhausted loop target");

        assert_eq!(target.target_node_id, "done");
        let reloaded_run = project_store::get_workflow_run(temp.path(), "project-1", &run.id)
            .expect("reload run")
            .expect("run exists");
        let retry_attempt = reloaded_run
            .loop_attempts
            .iter()
            .find(|attempt| attempt.loop_key == "retry")
            .expect("retry attempt exists");
        assert_eq!(retry_attempt.attempt_count, 2);
        assert!(retry_attempt.exhausted);
        let replay = replay_workflow_events(&reloaded_run);
        assert_eq!(replay.loop_exhaustions, 1);
    }
}
